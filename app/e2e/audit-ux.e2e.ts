// Guards for the 2026-11-15 audit's UX findings: undo from a field after
// Enter, draw-bar defaults said to be app-wide, half-hip defaults under the
// ridge, undo after closing a footprint, wall length keeping the angles,
// file names for the save dialog, offer figures in the library, unusual
// values marked, the trash with Restore, and shortcut hints on a Mac.
import { bar, clickPlan, openEditor, planXY, selectWall, setField, tool } from "./editor";
import { bridge, createExample, expect, listProjects, loadProject, test, waitForProject } from "./fixtures";
import { cs } from "./i18n";

const calc = (project: any) => bridge<any>("calc", { project });

test.describe("audit 2026-11-15 UX", () => {
  test("Ctrl+Z right after Enter in a wall field undoes the stored change (UX-001)", async ({ app, page }) => {
    const id = await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    await selectWall(page, 0);
    const overhang = bar(page, "def.title").getByLabel(cs("edge.overhang"));
    await overhang.fill("5");
    await overhang.press("Enter");
    await waitForProject(id, (p) => p.parts[0].edges[0].overhang === 5);
    // The focus is still in the field, nothing typed since Enter.
    await expect(overhang).toBeFocused();
    await overhang.press("Control+z");
    await waitForProject(id, (p) => p.parts[0].edges[0].overhang === 0.5);
    await expect(overhang).toHaveValue("0,5");
    // Redo brings it back, still from the field.
    await overhang.press("Control+Shift+z");
    await waitForProject(id, (p) => p.parts[0].edges[0].overhang === 5);
    await expect(overhang).toHaveValue("5");
  });

  test("while typing, Ctrl+Z stays the field's own text undo (UX-001)", async ({ app, page }) => {
    const id = await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    await selectWall(page, 0);
    const pitch = bar(page, "def.title").getByLabel(cs("edge.pitch"));
    await setField(pitch, 30);
    await waitForProject(id, (p) => p.parts[0].edges[0].pitch_deg === 30);
    await pitch.fill("44");
    await pitch.press("Control+z");
    await page.waitForTimeout(700);
    // The stored pitch is not undone by a Ctrl+Z meant for the typing.
    expect((await loadProject(id)).parts[0].edges[0].pitch_deg).toBe(30);
  });

  test("the draw bar says its defaults apply to every new building (UX-002)", async ({ app, page }) => {
    await createExample("gable", "Sedlo");
    await openEditor(app, "Sedlo");
    await page.locator("body").press("Insert");
    const draw = bar(page, "draw.title");
    await expect(draw).toBeVisible();
    await expect(draw.getByTestId("draw-presets-note")).toHaveText(cs("draw.presetsNote"));
    await expect(draw.getByRole("group", { name: cs("draw.presets") })).toHaveAttribute("title", cs("draw.presetsHint"));
  });

  test("a new half-hip starts below the ridge; one above it is marked (UX-003)", async ({ app, page }) => {
    const id = await createExample("gable", "Sedlo");
    const ridge = (await calc(await loadProject(id))).totals.ridge_height;
    await openEditor(app, "Sedlo");
    await selectWall(page, 1);
    const def = bar(page, "def.title");
    await def.getByLabel(cs("edge.halfHip")).check();
    const p = await waitForProject(id, (p) => !!p.parts[0].edges[1].half_hip);
    const h = p.parts[0].edges[1].half_hip.height;
    expect(h).toBeGreaterThan(p.parts[0].edges[1].eave_height);
    expect(h).toBeLessThan(ridge);
    // The roof changes: the half-hip has an effect.
    expect((await calc(p)).totals.roof_area).not.toBeCloseTo((await calc({ ...p, parts: [{ ...p.parts[0], edges: p.parts[0].edges.map((e: any) => ({ ...e, half_hip: null })) }] })).totals.roof_area, 2);
    await expect(def.getByTestId("halfhip-above-ridge")).toHaveCount(0);
    await setField(def.getByLabel(cs("edge.halfHipHeight")), ridge + 2);
    await expect(def.getByTestId("halfhip-above-ridge")).toBeVisible();
    await expect(page.locator(".results .warn")).toContainText(cs("res.warn.half_hip_above_ridge", { n: 2, v: "" }).split("(")[0].trim());
  });

  test("undo right after closing a footprint reopens it with its corners (UX-004)", async ({ app, page }) => {
    await app.open();
    await page.locator("header").getByRole("button", { name: cs("lib.new") }).click();
    const dlg = page.getByRole("dialog", { name: cs("new.title") });
    await dlg.getByLabel(cs("job.name")).fill("Kreslená");
    await dlg.getByRole("tab", { name: cs("new.draw") }).click();
    await dlg.getByRole("button", { name: cs("new.create") }).click();
    const draw = bar(page, "draw.title");
    await expect(draw).toBeVisible();
    const id = (await listProjects())[0].id as string;
    await clickPlan(page, 0, 0);
    const len = draw.getByLabel(cs("draw.length"));
    await len.fill("8");
    await len.press("ArrowRight");
    await len.press("ArrowUp");
    await len.press("ArrowLeft");
    await len.press("Enter");
    await waitForProject(id, (p) => p.parts.length === 1);
    await expect(draw).toHaveCount(0);
    await page.locator("body").press("Control+z");
    // The outline is open again with its four corners, not an empty plan.
    await expect(draw).toBeVisible();
    await expect(draw).toContainText(cs("draw.points", { n: 4 }).split("·")[0].trim());
    await waitForProject(id, (p) => p.parts.length === 0);
    // Closing again makes the same building.
    await draw.getByTestId("draw-close").click();
    const p = await waitForProject(id, (p) => p.parts.length === 1);
    expect(p.parts[0].edges).toHaveLength(4);
    // Cancel on an empty job says how to start instead of "pick a wall".
    await page.locator("body").press("Control+z");
    await expect(draw).toBeVisible();
    await draw.getByRole("button", { name: cs("draw.cancel") }).click();
    await expect(bar(page, "def.title").getByTestId("empty-tip")).toHaveText(cs("editor.empty"));
    await bar(page, "def.title").getByRole("button", { name: cs("tool.draw") }).click();
    await expect(draw).toBeVisible();
  });

  test("a typed wall length keeps a rectangle a rectangle (UX-005)", async ({ app, page }) => {
    const id = await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    await selectWall(page, 0);
    await setField(bar(page, "def.title").getByLabel(cs("edge.length")), 13);
    const p = await waitForProject(id, (p) => p.parts[0].edges[1].start.x === 13);
    expect(p.parts[0].edges.map((e: any) => [e.start.x, e.start.y])).toEqual([
      [0, 0],
      [13, 0],
      [13, 8],
      [0, 8],
    ]);
  });

  test("Export job proposes a file name both systems take (UX-006)", async ({ app, page }) => {
    await createExample("gable", "Novák / Brno 2026:117");
    await app.open();
    await app.openJob("Novák / Brno 2026:117");
    await page.evaluate(() => {
      window.prompt = (_m?: string, d?: string) => {
        (window as any).__savePath = d;
        return null;
      };
    });
    await page.locator("aside .tabs").getByRole("tab", { name: cs("dock.job") }).click();
    await page.getByRole("button", { name: cs("files.export") }).click();
    await expect.poll(() => page.evaluate(() => (window as any).__savePath)).toBe("Novák _ Brno 2026_117.tectorajob");
  });

  test("an offer lists its stored area and says when the roof changed (UX-008)", async ({ app, page }) => {
    const id = await createExample("gable", "Nabídka");
    const p = await loadProject(id);
    p.job.status = "offer";
    p.result_snapshot = await bridge("take_snapshot", { project: p });
    const stored = p.result_snapshot.roof_area;
    p.parts[0].edges[0].pitch_deg = 45;
    await bridge("save_project", { id, project: p });
    await app.open();
    const row = page.getByRole("row").filter({ hasText: "Nabídka" }).first();
    const cell = row.getByTestId("offer-area");
    await expect(cell).toContainText(stored.toLocaleString("cs-CZ", { minimumFractionDigits: 2, maximumFractionDigits: 2 }));
    await expect(cell).toContainText(cs("lib.differs"));
    await row.click();
    await expect(page.getByTestId("detail-differs")).toHaveText(cs("lib.differs"));
  });

  test("an overhang of 5 m is stored but marked, in the bar and the results (UX-009)", async ({ app, page }) => {
    const id = await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    await selectWall(page, 1);
    const def = bar(page, "def.title");
    await expect(def.getByTestId("unusual-overhang")).toHaveCount(0);
    await setField(def.getByLabel(cs("edge.overhang")), 5);
    await waitForProject(id, (p) => p.parts[0].edges[1].overhang === 5);
    await expect(def.getByTestId("unusual-overhang")).toHaveText(cs("edge.unusual"));
    await expect(page.locator(".results .warn")).toContainText(cs("res.warn.unusual_overhang", { n: 2, v: "5,00" }));
  });

  test("a deleted job comes back from the toast and from the trash (UX-010)", async ({ app, page }) => {
    await createExample("gable", "Smazat mě");
    await app.open();
    const row = () => page.getByRole("row").filter({ hasText: "Smazat mě" });
    await row().first().click();
    await app.answerConfirm(true);
    await page.locator("aside.detail").getByRole("button", { name: cs("card.delete") }).click();
    await expect(row()).toHaveCount(0);
    const toast = app.toasts().filter({ hasText: cs("home.deleted", { name: "Smazat mě" }) });
    await toast.getByRole("button", { name: cs("home.restore") }).click();
    await expect(row()).toHaveCount(1);
    expect((await listProjects()).map((p: any) => p.name)).toContain("Smazat mě");
    // Again, this time from the library's Trash.
    await row().first().click();
    await app.answerConfirm(true);
    await page.locator("aside.detail").getByRole("button", { name: cs("card.delete") }).click();
    await expect(row()).toHaveCount(0);
    await page.getByTestId("lib-trash").click();
    const list = page.getByTestId("trash-list");
    await expect(list).toContainText("Smazat mě");
    await list.getByRole("listitem").filter({ hasText: "Smazat mě" }).getByRole("button", { name: cs("home.restore") }).click();
    await expect.poll(async () => (await listProjects()).map((p: any) => p.name)).toContain("Smazat mě");
    await expect(page.getByTestId("lib-trash")).toContainText("0");
  });

  test("a Mac gets ⌘ and ⌫ hints and a draw shortcut it has (UX-011)", async ({ app, page }) => {
    await page.addInitScript(() => Object.defineProperty(navigator, "platform", { get: () => "MacIntel" }));
    await createExample("gable", "Sedlo");
    await openEditor(app, "Sedlo");
    await expect(tool(page, "tool.draw")).toHaveAttribute("title", /⌘B/);
    await expect(tool(page, "tool.draw")).not.toHaveAttribute("title", /Insert/);
    const at = await planXY(page, -2, -2);
    await page.mouse.click(at.x, at.y, { button: "right" });
    const draw = page.getByRole("menuitem", { name: cs("tool.draw") });
    await expect(draw.locator("kbd")).toHaveText("⌘B");
    await page.keyboard.press("Escape");
    await page.locator("body").press("Meta+b");
    await expect(bar(page, "draw.title")).toBeVisible();
  });
});
