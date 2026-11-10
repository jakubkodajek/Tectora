// Clarity changes from the 2026-11-10 review: the wall and dormer bars show
// the everyday fields and keep the rest behind "More" (UX-008, UI-008).
import { bar, openEditor, selectWall } from "./editor";
import { bridge, createExample, expect, loadProject, test, waitForProject } from "./fixtures";
import { cs } from "./i18n";

test.describe("wall bar More row", () => {
  test("shows only the everyday fields until More is opened, and marks what is set there", async ({ app, page }) => {
    await page.setViewportSize({ width: 960, height: 540 });
    const id = await createExample("gable", "Sedlo");
    const p = await loadProject(id);
    const eave = p.parts[0].edges.findIndex((e: any) => e.pitch_deg < 90);
    p.parts[0].edges[eave].breaks = [{ height: p.parts[0].edges[eave].eave_height + 1.5, pitch_deg: 25 }];
    p.parts[0].edges[eave].priority = 1;
    await bridge("save_project", { id, project: p });
    await app.open({ prefs: { wallMore: false } });
    await app.openJob("Sedlo");
    await selectWall(page, eave);
    const def = bar(page, "def.title");

    for (const key of ["edge.pitch", "edge.buildingHeight", "edge.overhang"]) await expect(def.getByText(cs(key), { exact: true })).toBeVisible();
    await expect(def.getByRole("radio", { name: cs("edge.eave") })).toBeVisible();
    for (const key of ["edge.length", "edge.startX", "edge.eaveHeight", "edge.material"]) await expect(def.getByText(cs(key), { exact: true })).toHaveCount(0);
    await expect(def.getByText(cs("edge.priority"), { exact: true })).toHaveCount(0);

    // The closed row says that a pitch break and the priority are set there.
    const more = page.getByTestId("def-more");
    await expect(more).toHaveAttribute("aria-expanded", "false");
    await expect(page.getByTestId("def-more-count")).toHaveText("2");
    await expect(more).toHaveAttribute("title", new RegExp(cs("edge.priority")));

    // At 960x540 the closed bar is one row, the open one two (no wrap to three).
    const h1 = (await def.boundingBox())!.height;
    expect(h1).toBeLessThan(100);
    await more.click();
    await expect(more).toHaveAttribute("aria-expanded", "true");
    await expect(def.getByText(cs("edge.breakHeight", { n: 1 }))).toBeVisible();
    await expect(def.getByText(cs("edge.eaveHeight"), { exact: true })).toBeVisible();
    const h2 = (await def.boundingBox())!.height;
    expect(h2).toBeLessThan(180);
    expect(await def.locator(".row").count()).toBe(2);

    // Remembered for the user: still open after a reload.
    await page.reload();
    await app.openJob("Sedlo");
    await selectWall(page, eave);
    await expect(page.getByTestId("def-more")).toHaveAttribute("aria-expanded", "true");
    // And by keyboard: Enter on the focused toggle closes it.
    await page.getByTestId("def-more").focus();
    await page.keyboard.press("Enter");
    await expect(page.getByTestId("def-more")).toHaveAttribute("aria-expanded", "false");
    await expect(def.getByText(cs("edge.eaveHeight"), { exact: true })).toHaveCount(0);
  });

  for (const locale of ["cs", "sk", "en", "de"] as const) {
    test(`the closed wall bar fits one row at 960x540 without scrolling (${locale})`, async ({ app, page }) => {
      await page.setViewportSize({ width: 960, height: 540 });
      const id = await createExample("hip", "Valba");
      await app.open({ locale, prefs: { wallMore: false } });
      await app.openJob("Valba");
      await page.locator(".view .wrap > svg g.edge line.hit").first().dispatchEvent("pointerdown", { button: 0 });
      const row = page.getByTestId("def-more").locator("xpath=..");
      await expect(page.getByTestId("def-more")).toBeVisible();
      const { client, scroll } = await row.evaluate((r) => ({ client: r.clientWidth, scroll: r.scrollWidth }));
      expect(scroll, `row overflows by ${scroll - client}px`).toBeLessThanOrEqual(client);
      expect((await loadProject(id)).parts).toHaveLength(1);
    });
  }

  test("the dormer bar keeps position and material behind More", async ({ app, page }) => {
    await page.setViewportSize({ width: 960, height: 540 });
    const id = await createExample("gable", "Sedlo");
    await app.open({ prefs: { dormerMore: false } });
    await app.openJob("Sedlo");
    await page.getByRole("toolbar").first().getByRole("button", { name: cs("tool.dormer"), exact: true }).click();
    const plan = page.locator(".view .wrap > svg").first();
    const box = (await plan.boundingBox())!;
    await page.mouse.click(box.x + box.width * 0.4, box.y + box.height * 0.35);
    const db = bar(page, "dormer.title");
    await expect(db.locator(".which strong")).toHaveText(cs("dormer.n", { n: 1 }));
    await expect(db.getByLabel(cs("dormer.width"))).toBeVisible();
    await expect(db.getByText(cs("edge.material"), { exact: true })).toHaveCount(0);
    expect((await db.boundingBox())!.height).toBeLessThan(100);
    // One row without sideways scrolling, the delete still named.
    const row = db.locator(".row").first();
    const { client, scroll } = await row.evaluate((r) => ({ client: r.clientWidth, scroll: r.scrollWidth }));
    expect(scroll, `dormer row overflows by ${scroll - client}px`).toBeLessThanOrEqual(client);
    await expect(db.getByRole("button", { name: cs("dormer.delete"), exact: true })).toBeVisible();
    await page.getByTestId("dormer-more").click();
    await expect(db.getByText(cs("edge.material"), { exact: true })).toBeVisible();
    expect((await db.boundingBox())!.height).toBeLessThan(180);
    await waitForProject(id, (p) => p.dormers.length === 1);
  });
});

test.describe("one word for a building", () => {
  test("a job from a template shows its building as Budova 1, not the core's Main (UX-009, UX-010)", async ({ app, page }) => {
    const id = await createExample("gable", "Sedlo");
    const p = await loadProject(id);
    expect(p.parts[0].name).toBe("Main");
    const second = JSON.parse(JSON.stringify(p.parts[0]));
    // A name the editor stored before the change, in its old wording.
    second.name = "Část 2";
    for (const e of second.edges) e.start.x += 30;
    p.parts.push(second);
    await bridge("save_project", { id, project: p });
    await openEditor(app, "Sedlo");
    const results = page.locator(".results");
    await expect(results).toContainText(cs("editor.partN", { n: 1 }));
    await expect(results).toContainText(cs("editor.partN", { n: 2 }));
    await expect(results).not.toContainText("Main");
    await expect(page.locator(".statusbar")).toContainText(cs("status.walls", { n: 8, p: 2 }));
    await expect(page.getByRole("toolbar").first().getByRole("button", { name: cs("tool.deletePart"), exact: true })).toBeVisible();
    await selectWall(page, 0);
    await expect(bar(page, "def.title").locator(".which span")).toContainText(cs("editor.partN", { n: 1 }));
    // The stored names stay as they are; only the screen localizes them.
    await expect(page.locator(".statusbar")).not.toContainText("Část");
    expect((await loadProject(id)).parts.map((x: any) => x.name)).toEqual(["Main", "Část 2"]);
  });
});

test.describe("back, undo and delete say what they do", () => {
  test("Back goes to the job list, undo is the only Zpět, every trash names its scope (UX-020, UI-006, UI-013)", async ({ app, page }) => {
    const id = await createExample("gable", "Sedlo");
    await openEditor(app, "Sedlo");
    // The Czech toolbar keeps its text labels at 1360x860 (the default size).
    await expect(page.locator(".toolbar")).not.toHaveClass(/compact/);
    await expect(page.getByRole("toolbar").first().getByText(cs("tool.deletePart"), { exact: true })).toBeVisible();
    const back = page.locator("header").getByRole("button", { name: cs("editor.back"), exact: true });
    await expect(back).toHaveAttribute("title", cs("editor.back"));
    expect(cs("editor.back")).not.toContain(cs("editor.undo"));
    expect(cs("draw.undo")).not.toContain(cs("editor.undo"));

    // Distinct names for the 3D layout, the 3D camera preset and the two fits.
    const names = [cs("editor.model"), cs("view.iso"), cs("plan.fit"), cs("view.fit")];
    expect(new Set(names).size).toBe(4);
    await expect(page.getByRole("toolbar").first().getByRole("button", { name: cs("editor.model"), exact: true })).toHaveAttribute("title", `${cs("editor.modelHint")} (F3)`);
    await expect(page.locator(".view").last().getByRole("button", { name: cs("view.fit"), exact: true })).toHaveAttribute("title", cs("view.fitHint"));

    // The corner delete is a labelled button in the More row.
    await selectWall(page, 0);
    const def = bar(page, "def.title");
    await expect(def.getByRole("button", { name: cs("edge.deleteVertex"), exact: true })).toHaveAttribute("title", cs("edge.deleteVertexHint"));

    // Deleting a dormer asks; No keeps it.
    await page.getByRole("toolbar").first().getByRole("button", { name: cs("tool.dormer"), exact: true }).click();
    const box = (await page.locator(".view .wrap > svg").first().boundingBox())!;
    await page.mouse.click(box.x + box.width * 0.4, box.y + box.height * 0.35);
    await waitForProject(id, (p) => p.dormers.length === 1);
    const db = bar(page, "dormer.title");
    const del = db.getByRole("button", { name: cs("dormer.delete"), exact: true });
    await expect(del).toBeVisible();
    await app.answerConfirm(false);
    await del.click();
    await expect.poll(async () => (await app.dialogs()).map((d) => d.message)).toContain(cs("dormer.confirmDelete", { n: 1 }));
    await page.waitForTimeout(700);
    expect((await loadProject(id)).dormers).toHaveLength(1);
    await app.answerConfirm(true);
    await del.click();
    await waitForProject(id, (p) => p.dormers.length === 0);

    // Deleting the building asks too.
    await selectWall(page, 0);
    await app.answerConfirm(false);
    await page.getByRole("toolbar").first().getByRole("button", { name: cs("tool.deletePart"), exact: true }).click();
    await expect.poll(async () => (await app.dialogs()).filter((d) => d.kind === "confirm").length).toBe(3);
    await page.waitForTimeout(700);
    expect((await loadProject(id)).parts).toHaveLength(1);
  });
});
