// Job library (Home): new job, examples, search, filters, sort, list / grid,
// detail pane, open, duplicate, delete, import and the broken-file notice.
import fs from "node:fs";
import path from "node:path";
import { LIBRARY_DIR } from "./env";
import { createExample, expect, listProjects, loadProject, scratchFile, test, waitForProject, bridge } from "./fixtures";
import { cs, re } from "./i18n";

test.describe("library", () => {
  test("empty library offers a new job", async ({ app, page }) => {
    await app.open();
    await expect(page.getByText(cs("home.empty"))).toBeVisible();
    await page.locator("main .empty").getByRole("button", { name: cs("lib.new") }).click();
    await expect(page.getByRole("dialog", { name: cs("new.title") })).toBeVisible();
  });

  test("new job from a standard roof stores the job data and opens the editor", async ({ app, page }) => {
    await app.open();
    await page.locator("header").getByRole("button", { name: cs("lib.new") }).click();
    const dlg = page.getByRole("dialog", { name: cs("new.title") });
    await dlg.getByLabel(cs("job.name")).fill("Novák – garáž");
    await dlg.getByLabel(cs("job.number")).fill("Z-2026-001");
    await dlg.getByLabel(cs("job.customer")).fill("Jan Novák");
    await dlg.getByLabel(cs("job.address")).fill("Hlavní 1, Brno");
    await dlg.getByRole("button", { name: cs("new.create") }).click();
    await expect(page.locator(".editor .statusbar")).toBeVisible();
    await expect(page.locator(".editor input.title")).toHaveValue("Novák – garáž");
    await app.saved();
    const [p] = await listProjects();
    expect(p.name).toBe("Novák – garáž");
    expect(p.job).toMatchObject({ number: "Z-2026-001", customer: "Jan Novák", address: "Hlavní 1, Brno", status: "draft" });
    expect(p.parts).toBe(1);
    expect(p.roof_area).toBeGreaterThan(10);
  });
});

test.describe("library actions", () => {
  test.beforeEach(async ({ app: _app }) => {
    const a = await createExample("gable", "Sedlo Novák");
    const b = await createExample("hip", "Valba Dvořák");
    const c = await createExample("l-shape", "Elko Šťastný");
    const set = async (id: string, job: Record<string, string>) => {
      const p = await loadProject(id);
      p.job = { ...p.job, ...job };
      await bridge("save_project", { id, project: p });
    };
    await set(a, { number: "Z-003", customer: "Novák", status: "offer" });
    await set(b, { number: "Z-001", customer: "Dvořák", status: "ordered", address: "Brno" });
    await set(c, { number: "Z-002", customer: "Šťastný" });
  });

  const names = async (page: import("@playwright/test").Page) =>
    page.locator("main table tbody tr td:nth-child(2) strong").allTextContents();

  test("search ignores case and diacritics and matches customer, number and address", async ({ app, page }) => {
    await app.open();
    const search = page.getByPlaceholder(cs("lib.search"));
    await search.fill("stastny");
    await expect.poll(() => names(page)).toEqual(["Elko Šťastný"]);
    await search.fill("z-001");
    await expect.poll(() => names(page)).toEqual(["Valba Dvořák"]);
    await search.fill("BRNO");
    await expect.poll(() => names(page)).toEqual(["Valba Dvořák"]);
    await search.fill("nic takového");
    await expect(page.getByText(cs("home.noMatch", { q: "nic takového" }))).toBeVisible();
  });

  test("status filters show the right jobs and counts", async ({ app, page }) => {
    await app.open();
    const nav = page.locator("nav");
    await expect(nav.getByRole("button", { name: cs("lib.all") })).toContainText("3");
    await nav.getByRole("button", { name: cs("status.offer") }).click();
    await expect.poll(() => names(page)).toEqual(["Sedlo Novák"]);
    await nav.getByRole("button", { name: cs("status.ordered") }).click();
    await expect.poll(() => names(page)).toEqual(["Valba Dvořák"]);
    await nav.getByRole("button", { name: cs("status.draft") }).click();
    await expect.poll(() => names(page)).toEqual(["Elko Šťastný"]);
    await nav.getByRole("button", { name: cs("status.done") }).click();
    await expect(page.locator("main table")).toHaveCount(0);
    await nav.getByRole("button", { name: cs("lib.byAi") }).click();
    await expect(page.locator("main table")).toHaveCount(0);
    await nav.getByRole("button", { name: cs("lib.all") }).click();
    await expect.poll(async () => (await names(page)).length).toBe(3);
  });

  test("column headers sort both ways", async ({ app, page }) => {
    await app.open();
    const head = page.locator("main thead");
    await head.getByRole("button", { name: cs("lib.colName") }).click();
    await expect.poll(() => names(page)).toEqual(["Elko Šťastný", "Sedlo Novák", "Valba Dvořák"]);
    await head.getByRole("button", { name: cs("lib.colName") }).click();
    await expect.poll(() => names(page)).toEqual(["Valba Dvořák", "Sedlo Novák", "Elko Šťastný"]);
    await head.getByRole("button", { name: cs("job.number") }).click();
    await expect.poll(() => names(page)).toEqual(["Valba Dvořák", "Elko Šťastný", "Sedlo Novák"]);
    await head.getByRole("button", { name: cs("job.customer") }).click();
    await expect.poll(() => names(page)).toEqual(["Valba Dvořák", "Sedlo Novák", "Elko Šťastný"]);
    await head.getByRole("button", { name: cs("lib.colArea") }).click();
    const areas = await page.locator("main tbody td.num").allTextContents();
    const nums = areas.map((a) => Number(a.replace(/[^\d,]/g, "").replace(",", ".")));
    expect(nums).toEqual([...nums].sort((a, b) => b - a));
    await head.getByRole("button", { name: cs("lib.colModified") }).click();
    await expect(head.getByRole("button", { name: cs("lib.colModified") })).toContainText("↓");
  });

  test("detail pane: select, open, duplicate, delete with confirmation", async ({ app, page }) => {
    await app.open();
    await page.getByRole("row").filter({ hasText: "Valba Dvořák" }).click();
    const detail = page.locator("aside.detail");
    await expect(detail.getByRole("heading", { name: "Valba Dvořák" })).toBeVisible();
    await expect(detail).toContainText("Z-001");
    await expect(detail).toContainText("Brno");

    await detail.getByRole("button", { name: cs("card.duplicate") }).click();
    await expect(page.getByRole("row").filter({ hasText: cs("lib.copyName", { name: "Valba Dvořák" }) })).toBeVisible();
    await expect.poll(async () => (await listProjects()).length).toBe(4);
    // The copy is selected.
    await expect(detail.getByRole("heading")).toHaveText(cs("lib.copyName", { name: "Valba Dvořák" }));

    // Cancel keeps the job.
    await app.answerConfirm(false);
    await detail.getByRole("button", { name: cs("card.delete") }).click();
    await expect.poll(async () => (await listProjects()).length).toBe(4);
    expect((await app.dialogs()).at(-1)?.message).toBe(cs("home.confirmDelete", { name: cs("lib.copyName", { name: "Valba Dvořák" }) }));

    await app.answerConfirm(true);
    await detail.getByRole("button", { name: cs("card.delete") }).click();
    await expect.poll(async () => (await listProjects()).length).toBe(3);
    await expect(detail).toHaveCount(0);

    await page.getByRole("row").filter({ hasText: "Sedlo Novák" }).click();
    await detail.getByRole("button", { name: cs("card.open") }).click();
    await expect(page.locator(".editor input.title")).toHaveValue("Sedlo Novák");
  });

  test("grid view: cards open, duplicate and delete from the menu; view choice persists", async ({ app, page }) => {
    await app.open();
    await page.getByRole("button", { name: cs("lib.grid") }).click();
    const card = page.locator("article.card").filter({ has: page.getByRole("heading", { name: "Elko Šťastný", exact: true }) });
    await expect(card).toBeVisible();
    await card.getByRole("button", { name: cs("card.more") }).click();
    await card.getByRole("menuitem", { name: cs("card.duplicate") }).click();
    await expect(page.locator("article.card")).toHaveCount(4);
    // The menu closes on any click elsewhere.
    await card.getByRole("button", { name: cs("card.more") }).click();
    await expect(card.getByRole("menu")).toBeVisible();
    await page.locator("main .head h2").click();
    await expect(card.getByRole("menu")).toHaveCount(0);

    await app.answerConfirm(true);
    const copy = page.locator("article.card").filter({ hasText: cs("lib.copyName", { name: "Elko Šťastný" }) });
    await copy.getByRole("button", { name: cs("card.more") }).click();
    await copy.getByRole("menuitem", { name: cs("card.delete") }).click();
    await expect(page.locator("article.card")).toHaveCount(3);

    await page.reload();
    await expect(page.locator("article.card")).toHaveCount(3);
    await card.getByRole("button", { name: `${cs("card.open")} Elko Šťastný` }).click();
    await expect(page.locator(".editor input.title")).toHaveValue("Elko Šťastný");
    await page.getByRole("button", { name: cs("editor.back") }).click();
    await page.getByRole("button", { name: cs("lib.list") }).click();
    await expect(page.locator("main table")).toBeVisible();
  });

  test("double-click on a row opens the job; back returns to the library", async ({ app, page }) => {
    await app.open();
    await app.openJob("Elko Šťastný");
    await page.locator(".crumbs").getByRole("button", { name: cs("lib.title") }).click();
    await expect(page.locator("main table")).toBeVisible();
    await app.openJob("Elko Šťastný");
    await page.getByRole("button", { name: cs("editor.back") }).click();
    await expect(page.locator("main table")).toBeVisible();
  });
});

/** A job of `n` separate 10 x 8 m buildings. */
function manyParts(n: number): string {
  const edge = (x: number, y: number) => ({ start: { x, y }, pitch_deg: 35, eave_height: 3, overhang: 0.5 });
  const parts = Array.from({ length: n }, (_, i) => {
    const [x, y] = [(i % 20) * 15, Math.floor(i / 20) * 12];
    return { name: `P${i}`, edges: [edge(x, y), edge(x + 10, y), edge(x + 10, y + 8), edge(x, y + 8)] };
  });
  return JSON.stringify({ schema_version: 2, name: `parts-${n}`, parts });
}

test.describe("library: examples, new job tabs, import, broken files", () => {
  test("examples gallery starts a new job from the chosen example", async ({ app, page }) => {
    await app.open();
    await page.locator("nav").getByRole("button", { name: cs("home.examples") }).click();
    await page.locator(".examples button.example").filter({ has: page.getByText(cs("ex.hip.name"), { exact: true }) }).click();
    const dlg = page.getByRole("dialog", { name: cs("new.title") });
    await expect(dlg.getByRole("tab", { name: cs("new.example") })).toHaveAttribute("aria-selected", "true");
    await dlg.getByLabel(cs("job.customer")).fill("Zákazník X");
    await dlg.getByRole("button", { name: cs("new.create") }).click();
    await expect(page.locator(".editor input.title")).toHaveValue(cs("ex.hip.name"));
    const [p] = await listProjects();
    expect(p.job.customer).toBe("Zákazník X");
    expect(p.lines.length).toBeGreaterThan(4);
  });

  test("new job dialog: cancel, close, Escape and backdrop close without creating", async ({ app, page }) => {
    await app.open();
    const open = () => page.locator("header").getByRole("button", { name: cs("lib.new") }).click();
    const dlg = page.getByRole("dialog", { name: cs("new.title") });
    await open();
    await dlg.getByRole("button", { name: cs("common.cancel") }).click();
    await expect(dlg).toHaveCount(0);
    await open();
    await dlg.getByRole("button", { name: cs("common.close") }).click();
    await expect(dlg).toHaveCount(0);
    await open();
    await page.keyboard.press("Escape");
    await expect(dlg).toHaveCount(0);
    await open();
    await page.mouse.click(5, 5);
    await expect(dlg).toHaveCount(0);
    expect(await listProjects()).toEqual([]);
  });

  test("new job: every standard roof type creates a roof", async ({ app, page }) => {
    await app.open();
    const kinds = ["gable", "hip", "shed", "pyramid", "half_hip", "polygon"];
    for (const k of kinds) {
      await page.locator("header").getByRole("button", { name: cs("lib.new") }).click();
      const dlg = page.getByRole("dialog", { name: cs("new.title") });
      await dlg.getByLabel(cs("job.name")).fill(`Typ ${k}`);
      await dlg.getByRole("radio", { name: cs(`std.${k}`), exact: true }).click();
      await expect(dlg.locator(".preview .area")).toBeVisible();
      await dlg.getByRole("button", { name: cs("new.create") }).click();
      await expect(page.locator(".editor input.title")).toHaveValue(`Typ ${k}`);
      await expect(page.locator(".results .hero strong")).not.toHaveText("—");
      await page.getByRole("button", { name: cs("editor.back") }).click();
    }
    const all = await listProjects();
    expect(all.map((p) => p.name).sort()).toEqual(kinds.map((k) => `Typ ${k}`).sort());
    for (const p of all) expect(p.roof_area, p.name).toBeGreaterThan(1);
  });

  test("new job from this roof shows the chosen roof and, without a name, names the copy apart (UX-016)", async ({ app, page }) => {
    for (let i = 1; i <= 9; i++) await createExample("gable", `Zdroj ${i}`);
    await app.open();
    const dlg = page.getByRole("dialog", { name: cs("new.title") });
    // The job at the end of the dialog's list, out of its view at first.
    await page.locator("header").getByRole("button", { name: cs("lib.new") }).click();
    await dlg.getByRole("tab", { name: cs("new.past") }).click();
    const last = ((await dlg.getByRole("option").last().locator("strong").textContent()) ?? "").trim();
    expect(last).toMatch(/^Zdroj \d$/);
    await dlg.getByRole("button", { name: cs("common.cancel") }).click();

    await page.getByRole("row").filter({ hasText: last }).first().click();
    await page.getByRole("button", { name: cs("card.reuse") }).first().click();
    await expect(dlg.getByRole("tab", { name: cs("new.past") })).toHaveAttribute("aria-selected", "true");
    await expect(dlg.getByTestId("past-chosen")).toContainText(last);
    // The preselected row is scrolled into the list's view.
    const chosen = dlg.getByRole("option", { selected: true });
    await expect(chosen).toContainText(last);
    await expect.poll(async () => {
      const list = (await dlg.getByRole("listbox").boundingBox())!;
      const row = (await chosen.boundingBox())!;
      return row.y >= list.y - 1 && row.y + row.height <= list.y + list.height + 1;
    }).toBe(true);
    const name = dlg.getByLabel(cs("job.name"));
    await expect(name).toHaveAttribute("placeholder", cs("lib.copyName", { name: last }));
    await dlg.getByRole("button", { name: cs("new.create") }).click();
    await expect(page.locator(".editor input.title")).toHaveValue(cs("lib.copyName", { name: last }));
    const names = (await listProjects()).map((p) => p.name);
    expect(names.filter((n) => n === last)).toHaveLength(1);
    expect(names).toContain(cs("lib.copyName", { name: last }));
  });

  test("new job by drawing opens an empty plan in draw mode", async ({ app, page }) => {
    await app.open();
    await page.locator("header").getByRole("button", { name: cs("lib.new") }).click();
    const dlg = page.getByRole("dialog", { name: cs("new.title") });
    await dlg.getByRole("tab", { name: cs("new.draw") }).click();
    await expect(dlg.getByText(cs("new.drawHint"))).toBeVisible();
    await dlg.getByRole("button", { name: cs("new.create") }).click();
    await expect(page.getByRole("toolbar", { name: cs("draw.title") })).toBeVisible();
    const [p] = await listProjects();
    expect(p.parts).toBe(0);
    expect(p.name).toBe(cs("home.newName"));
  });

  test("import a .tectorajob bundle exported from another job", async ({ app, page }) => {
    const id = await createExample("t-shape", "Na export");
    const file = scratchFile("bundle/Zakázka Nový Dům.tectorajob", "");
    await bridge("export_job", { id, path: file });
    await bridge("delete_project", { id });
    await app.open();
    await app.answerFileDialog(file);
    await page.getByRole("button", { name: cs("home.importJob") }).click();
    await expect(page.locator(".editor input.title")).toHaveValue("Na export");
    expect((await listProjects()).length).toBe(1);
  });

  test("import cancelled or of a damaged file changes nothing and explains why", async ({ app, page }) => {
    await app.open();
    await app.answerFileDialog(null);
    await page.getByRole("button", { name: cs("home.importJob") }).click();
    await expect(page.locator("main .empty")).toBeVisible();
    const bad = scratchFile("bad.tectorajob", "this is not a zip");
    await app.answerFileDialog(bad);
    await page.getByRole("button", { name: cs("home.importJob") }).click();
    await expect(app.toasts().first()).toBeVisible();
    await expect(app.toasts().first()).not.toContainText(/DACAD|undefined|\[object/);
    expect(await listProjects()).toEqual([]);
    app.errors.length = 0;
  });

  test("a refused import says why: newer Tectora, damaged file, not a Tectora file", async ({ app, page }) => {
    // ERR-007: every refused import read "An unexpected error occurred".
    await app.open();
    const cases: [string, string | Buffer, string][] = [
      ["newer.tectora", JSON.stringify({ schema_version: 999, name: "Z budoucnosti", parts: [] }), "err.newer_format"],
      ["rozbita.tectorajob", "this is not a zip", "err.unreadable"],
      ["vykres.pdf", "%PDF-1.4", "err.unsupported_type"],
      // ARCH-002: more buildings than any job has would block the library.
      ["mnoho.tectora", manyParts(201), "err.too_many"],
    ];
    for (const [name, content, key] of cases) {
      await app.answerFileDialog(scratchFile(name, content));
      await page.getByRole("button", { name: cs("home.importJob") }).click();
      const toast = app.toasts().filter({ hasText: cs(key) });
      await expect(toast, name).toBeVisible();
      await expect(toast).toContainText(cs("err.action.import"));
      await expect(toast).not.toContainText(cs("err.unknown"));
    }
    expect(await listProjects()).toEqual([]);
    app.errors.length = 0;
  });

  test("an unreadable file in the folder is reported, the rest still lists", async ({ app, page }) => {
    await createExample("gable", "Dobrá");
    fs.writeFileSync(path.join(LIBRARY_DIR, "rozbita.tectora"), "{ not json");
    await app.open();
    await expect(page.getByRole("row").filter({ hasText: "Dobrá" })).toBeVisible();
    const notice = page.locator(".broken");
    await expect(notice).toContainText(cs("lib.broken", { n: 1 }));
    await notice.getByRole("button", { name: cs("lib.brokenShow") }).click();
    await expect(notice).toContainText("rozbita.tectora");
    await notice.getByRole("button", { name: cs("err.hideDetails") }).click();
    await expect(notice).not.toContainText("rozbita.tectora");
  });

  test("library folder button and gear open the settings; AI button opens the connect dialog", async ({ app, page }) => {
    await app.open();
    await page.locator("nav button.folder").click();
    await expect(page.getByRole("dialog", { name: cs("settings.title") })).toBeVisible();
    await page.keyboard.press("Escape");
    await page.getByRole("button", { name: cs("settings.button") }).click();
    await expect(page.getByRole("dialog", { name: cs("settings.title") })).toBeVisible();
    await page.keyboard.press("Escape");
    await page.keyboard.press("Control+,");
    await expect(page.getByRole("dialog", { name: cs("settings.title") })).toBeVisible();
    await page.keyboard.press("Escape");
    await page.getByRole("button", { name: cs("ai.connect") }).click();
    await expect(page.getByRole("dialog", { name: cs("conn.title") })).toBeVisible();
  });

  test("jobs saved by another program appear without a reload", async ({ app, page }) => {
    await app.open();
    await expect(page.locator("main .empty")).toBeVisible();
    await createExample("gable", "Zvenku");
    await expect(page.getByRole("row").filter({ hasText: "Zvenku" })).toBeVisible();
  });
});
