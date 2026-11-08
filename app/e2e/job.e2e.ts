// Job tab: job data, status (and the stored offer result it takes), the
// stored-result comparison in Results, attachments, export, material names.
import fs from "node:fs";
import path from "node:path";
import { LIBRARY_DIR } from "./env";
import { bar, openEditor, selectWall, setField } from "./editor";
import { bridge, createExample, expect, listProjects, loadProject, scratchFile, test, waitForProject } from "./fixtures";
import { cs } from "./i18n";

async function openJobTab(app: import("./fixtures").App, name: string) {
  await openEditor(app, name);
  await app.page.locator("aside .tabs").getByRole("tab", { name: cs("dock.job") }).click();
  return app.page.locator("aside .job");
}

test.describe("job tab", () => {
  test("job fields are saved and shown in the library", async ({ app, page }) => {
    const id = await createExample("gable", "Sedlo");
    const job = await openJobTab(app, "Sedlo");
    await job.getByLabel(cs("job.name")).fill("Rodinný dům Kočí");
    await job.getByLabel(cs("job.name")).press("Tab");
    await job.getByLabel(cs("job.number")).fill("2026/117");
    await job.getByLabel(cs("job.number")).press("Tab");
    await job.getByLabel(cs("job.customer")).fill("Petr Kočí");
    await job.getByLabel(cs("job.customer")).press("Tab");
    await job.getByLabel(cs("job.address")).fill("Polní 5, Zlín");
    await job.getByLabel(cs("job.address")).press("Tab");
    await job.getByLabel(cs("job.notes")).fill("Krytina: Bramac, barva červená.\nLešení od 12. 11.");
    await job.getByLabel(cs("job.notes")).press("Tab");
    const p = await waitForProject(id, (p) => p.notes.startsWith("Krytina"));
    expect(p.name).toBe("Rodinný dům Kočí");
    expect(p.job).toMatchObject({ number: "2026/117", customer: "Petr Kočí", address: "Polní 5, Zlín" });
    await expect(page.locator(".editor input.title")).toHaveValue("Rodinný dům Kočí");
    await expect(page.locator(".crumbs .jobno")).toHaveText("2026/117");
    await page.getByRole("button", { name: cs("editor.back") }).click();
    await page.getByRole("row").filter({ hasText: "Rodinný dům Kočí" }).click();
    await expect(page.locator("aside.detail")).toContainText("Polní 5, Zlín");
    await expect(page.locator("aside.detail")).toContainText("Krytina: Bramac");
  });

  test("leaving draft stores the offer result; changes show as a difference; store current updates it", async ({ app, page }) => {
    const id = await createExample("gable", "Sedlo");
    const job = await openJobTab(app, "Sedlo");
    await job.getByRole("combobox", { name: cs("job.status") }).selectOption("offer");
    let p = await waitForProject(id, (p) => p.job.status === "offer" && !!p.result_snapshot);
    const storedArea = p.result_snapshot.roof_area;
    await page.locator("aside .tabs").getByRole("tab", { name: cs("dock.results") }).click();
    await expect(page.locator(".results .snap")).toContainText(cs("snap.same"));

    await selectWall(page, 0);
    await setField(bar(page, "def.title").getByLabel(cs("edge.pitch")), 45);
    await expect(page.locator(".results .snap")).toContainText(cs("snap.changed"));
    await expect(page.locator(".results .snap table.diff")).toContainText(cs("res.area"));
    await page.locator(".results").getByRole("button", { name: cs("snap.store") }).click();
    await expect(page.locator(".results .snap")).toContainText(cs("snap.same"));
    p = await waitForProject(id, (p) => p.result_snapshot.roof_area !== storedArea);
    expect(p.result_snapshot.roof_area).toBeGreaterThan(storedArea);

    // Back to draft and forward again keeps the stored result (no silent overwrite).
    await page.locator("aside .tabs").getByRole("tab", { name: cs("dock.job") }).click();
    await job.getByRole("combobox", { name: cs("job.status") }).selectOption("ordered");
    p = await waitForProject(id, (p) => p.job.status === "ordered");
    expect(p.result_snapshot).toBeTruthy();
  });

  test("attach files, cancel keeps a file, remove asks first", async ({ app, page }) => {
    const id = await createExample("gable", "Sedlo");
    const a = scratchFile("Výkres přízemí.pdf", "%PDF-1.4 test");
    const b = scratchFile("foto 1.jpg", Buffer.alloc(2048, 1));
    const job = await openJobTab(app, "Sedlo");
    await expect(job).toContainText(cs("files.empty"));
    await app.answerFileDialog(a);
    await job.getByRole("button", { name: cs("files.add") }).click();
    await expect(job.locator("ul.files li")).toHaveCount(1);
    await app.answerFileDialog(b);
    await job.getByRole("button", { name: cs("files.add") }).click();
    await expect(job.locator("ul.files li")).toHaveCount(2);
    await expect(job.locator("ul.files")).toContainText("Výkres přízemí.pdf");
    await expect(job.locator("ul.files")).toContainText("2 kB");
    expect(fs.readdirSync(path.join(LIBRARY_DIR, ".files", id)).sort()).toEqual(["foto 1.jpg", "Výkres přízemí.pdf"].sort());

    // Cancel in the confirmation keeps the file (Tauri's confirm is asynchronous).
    await app.answerConfirm(false);
    await job.locator("ul.files li").filter({ hasText: "foto 1.jpg" }).getByRole("button", { name: cs("files.remove") }).click();
    await page.waitForTimeout(700);
    await expect(job.locator("ul.files li")).toHaveCount(2);
    expect((await bridge<any[]>("list_attachments", { id })).length).toBe(2);

    await app.answerConfirm(true);
    await job.locator("ul.files li").filter({ hasText: "foto 1.jpg" }).getByRole("button", { name: cs("files.remove") }).click();
    await expect(job.locator("ul.files li")).toHaveCount(1);
    expect((await bridge<any[]>("list_attachments", { id })).map((f) => f.name)).toEqual(["Výkres přízemí.pdf"]);

    // Cancelling the file dialog adds nothing.
    await app.answerFileDialog(null);
    await job.getByRole("button", { name: cs("files.add") }).click();
    await expect(job.locator("ul.files li")).toHaveCount(1);
  });

  test("export the job to a .tectorajob with files and import it back", async ({ app, page }) => {
    const id = await createExample("l-shape", "Elko export");
    await bridge("attach_files", { id, paths: [scratchFile("plan.pdf", "%PDF")] });
    const job = await openJobTab(app, "Elko export");
    const out = path.join(path.dirname(scratchFile("x", "")), "Zakázky", "Nový Dům.tectorajob");
    fs.mkdirSync(path.dirname(out), { recursive: true });
    await app.answerFileDialog(out);
    await job.getByRole("button", { name: cs("files.export") }).click();
    await expect.poll(() => fs.existsSync(out)).toBe(true);
    expect(fs.statSync(out).size).toBeGreaterThan(200);

    // Leave the editor first: deleting the open job is its own test below,
    // and the watcher reports it as an error.
    await page.getByRole("button", { name: cs("editor.back") }).click();
    await expect(page.locator(".editor")).toHaveCount(0);
    await bridge("delete_project", { id });
    await app.answerFileDialog(out);
    await page.getByRole("button", { name: cs("home.importJob") }).click();
    await expect(page.locator(".editor input.title")).toHaveValue("Elko export");
    const [imported] = await listProjects();
    expect((await bridge<any[]>("list_attachments", { id: imported.id })).map((f) => f.name)).toEqual(["plan.pdf"]);
    expect(imported.roof_area).toBeGreaterThan(100);
  });

  test("export dialog: plan sheet as PDF and the price-sheet text with a copy button", async ({ app, page }) => {
    await createExample("hip", "Valba export");
    await openEditor(app, "Valba export");
    await page.getByRole("button", { name: cs("tool.export") }).click();
    const dlg = page.getByRole("dialog", { name: cs("export.title") });
    await expect(dlg.locator(".paper img")).toBeVisible();
    const list = dlg.locator("pre");
    await expect(list).toContainText("DACHLINIEN");
    await expect(list).toContainText("Valba export");
    await expect(list).toContainText("m\u02db");

    await page.context().grantPermissions(["clipboard-read", "clipboard-write"]);
    await dlg.getByRole("button", { name: cs("export.copyList") }).click();
    await expect(dlg.getByRole("button", { name: cs("export.copied") })).toBeVisible();
    expect(await page.evaluate(() => navigator.clipboard.readText())).toBe(await list.textContent());

    const out = path.join(path.dirname(scratchFile("x", "")), "Valba export.pdf");
    await app.answerFileDialog(out);
    await dlg.getByRole("button", { name: "PDF" }).click();
    await expect.poll(() => fs.existsSync(out)).toBe(true);
    expect(fs.readFileSync(out).subarray(0, 5).toString()).toBe("%PDF-");
    await dlg.getByRole("button", { name: cs("common.close") }).last().click();
    await expect(dlg).toHaveCount(0);
  });

  test("material names appear in the wall bar and in the results", async ({ app, page }) => {
    const id = await createExample("hip", "Valba");
    const job = await openJobTab(app, "Valba");
    const mats = job.locator("label.mat input");
    await expect(mats).toHaveCount(await mats.count());
    await mats.nth(1).fill("Plech Lindab");
    await mats.nth(1).press("Tab");
    await waitForProject(id, (p) => p.material_names?.[1] === "Plech Lindab");
    await selectWall(page, 0);
    const def = bar(page, "def.title");
    await def.getByLabel(cs("edge.material")).selectOption({ label: "Plech Lindab" });
    await waitForProject(id, (p) => p.parts[0].edges[0].material === 2);
    await page.locator("aside .tabs").getByRole("tab", { name: cs("dock.results") }).click();
    await expect(page.locator(".results")).toContainText("Plech Lindab");
    // Clearing the name falls back to the default name.
    await page.locator("aside .tabs").getByRole("tab", { name: cs("dock.job") }).click();
    await mats.nth(1).fill("");
    await mats.nth(1).press("Tab");
    await waitForProject(id, (p) => !p.material_names?.length);
  });

  test("a job deleted outside the app while open keeps the edits and offers to save", async ({ app, page }) => {
    const id = await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    fs.rmSync(path.join(LIBRARY_DIR, `${id}.tectora`));
    await selectWall(page, 0);
    await setField(bar(page, "def.title").getByLabel(cs("edge.pitch")), 30);
    // Saving recreates the file with the edit.
    await expect.poll(() => fs.existsSync(path.join(LIBRARY_DIR, `${id}.tectora`)), { timeout: 8000 }).toBe(true);
    expect((await loadProject(id)).parts[0].edges[0].pitch_deg).toBe(30);
    app.errors.length = 0;
  });

  test("leaving after a failed save asks first; staying keeps the edits", async ({ app, page }) => {
    const id = await createExample("hip", "Plný disk");
    await openEditor(app, "Plný disk");
    // The disk is full: every save fails.
    await page.route("**/save_project", (r) => r.fulfill({ contentType: "application/json", body: JSON.stringify({ err: { code: "disk_full" } }) }));
    await selectWall(page, 0);
    await setField(bar(page, "def.title").getByLabel(cs("edge.pitch")), 31);
    await expect(page.locator(".statusbar .save-failed")).toBeVisible({ timeout: 8000 });
    // Back: the save is tried again, fails, the user says "stay".
    await app.answerConfirm(false);
    await page.getByRole("button", { name: cs("editor.back") }).first().click();
    await expect.poll(async () => (await app.dialogs()).map((d) => d.message)).toContain(cs("editor.leaveUnsaved"));
    await expect(page.locator(".editor")).toBeVisible();
    // Disk freed: the next try saves the edit, and Back leaves without asking.
    await page.unroute("**/save_project");
    await page.getByRole("button", { name: cs("editor.back") }).first().click();
    await expect(page.locator("main")).toBeVisible();
    expect((await loadProject(id)).parts[0].edges[0].pitch_deg).toBe(31);
    expect((await app.dialogs()).filter((d) => d.message === cs("editor.leaveUnsaved"))).toHaveLength(1);
    app.errors.length = 0;
  });

  test("a job saved meanwhile by a newer Tectora is not overwritten by the open editor", async ({ app, page }) => {
    const id = await createExample("hip", "Novější");
    await openEditor(app, "Novější");
    const file = path.join(LIBRARY_DIR, `${id}.tectora`);
    const newer = JSON.stringify({ schema_version: 99, name: "Novější", job: { customer: "Kolega" }, future_field: 1 });
    fs.writeFileSync(file, newer);
    await selectWall(page, 0);
    await setField(bar(page, "def.title").getByLabel(cs("edge.pitch")), 30);
    await expect(page.locator(".toast").filter({ hasText: cs("err.newer_format") }).first()).toBeVisible({ timeout: 8000 });
    expect(fs.readFileSync(file, "utf8")).toBe(newer);
    app.errors.length = 0;
  });

  test("a job file damaged while it is open is repaired from the editor's copy (ERR-018)", async ({ app, page }) => {
    const id = await createExample("hip", "Poškozená");
    await openEditor(app, "Poškozená");
    const file = path.join(LIBRARY_DIR, `${id}.tectora`);
    fs.writeFileSync(file, "{ broken");
    await selectWall(page, 0);
    await setField(bar(page, "def.title").getByLabel(cs("edge.pitch")), 30);
    await expect.poll(() => {
      try {
        return JSON.parse(fs.readFileSync(file, "utf8")).parts[0].edges[0].pitch_deg;
      } catch {
        return null;
      }
    }, { timeout: 8000 }).toBe(30);
    expect(JSON.parse(fs.readFileSync(file, "utf8")).name).toBe("Poškozená");
    app.errors.length = 0;
  });

  test("a job that cannot be opened shows the reason and a way back", async ({ app, page }) => {
    const id = await createExample("gable", "Rozbitá");
    await app.open();
    fs.writeFileSync(path.join(LIBRARY_DIR, `${id}.tectora`), "{ broken");
    await page.getByRole("row").filter({ hasText: "Rozbitá" }).dblclick();
    const err = page.locator(".load-error");
    await expect(err).toBeVisible();
    await expect(err).not.toContainText(/DACAD|undefined|\[object/);
    await err.getByRole("button", { name: cs("editor.back") }).click();
    await expect(page.locator("main")).toBeVisible();
    app.errors.length = 0;
  });
});
