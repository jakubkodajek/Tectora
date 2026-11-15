// Edge-cases class 2 (TEST-003, TEST-007): after every operation that
// changes the job (undo, redo, an AI save, a merge on save, a standard roof,
// a manual face made a building, a move, a rotation, a mirrored copy, a
// window, chimney or dormer, flat roof on and off, deleting a part, Revert
// of an AI change, the round roof and the calculation helpers), the shown
// result equals the core's calculation of the saved file and the plan's roof
// lines follow, with no further action. The area alone does not change under
// a move or a rotation, so those cases compare the roof lines as well.
import type { Page } from "@playwright/test";
import { bar, clickPlan, confirmDormer, firstArea, openEditor, selectWall, setField, shownArea, tool, planXY } from "./editor";
import { bridge, createExample, expect, loadProject, test, waitForProject } from "./fixtures";
import { cs } from "./i18n";

async function matchesCore(page: Page, id: string, what: string) {
  await expect.poll(async () => {
    const r = await bridge<any>("calc", { project: await loadProject(id) });
    return Math.abs((await shownArea(page)) - r.totals.roof_area);
  }, { message: what, timeout: 8000 }).toBeLessThan(0.01);
}
const lines = (page: Page) => page.locator(".roofline").evaluateAll((ls) => ls.map((l) => [l.getAttribute("x1"), l.getAttribute("y1"), l.getAttribute("x2"), l.getAttribute("y2")].join(",")).sort().join(";"));
async function pressPlan(page: Page, x: number, y: number) { const p = await planXY(page, x, y); await page.mouse.click(p.x, p.y); }

test.describe("every edit recalculates", () => {
  test("undo and redo show the roof of each step", async ({ app, page }) => {
    const id = await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    await firstArea(page);
    await selectWall(page, 0);
    const pitch = bar(page, "def.title").getByLabel(cs("edge.pitch"));
    await setField(pitch, 20);
    await waitForProject(id, (p) => p.parts[0].edges[0].pitch_deg === 20);
    await matchesCore(page, id, "after edit");
    const l20 = await lines(page);
    await tool(page, "editor.undo").click();
    await waitForProject(id, (p) => p.parts[0].edges[0].pitch_deg === 35);
    await matchesCore(page, id, "after undo");
    await expect.poll(() => lines(page)).not.toBe(l20);
    await tool(page, "editor.redo").click();
    await waitForProject(id, (p) => p.parts[0].edges[0].pitch_deg === 20);
    await matchesCore(page, id, "after redo");
  });

  test("an AI save shows the new roof", async ({ app, page, mcp }) => {
    const id = await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    await firstArea(page);
    const before = await lines(page);
    const a0 = await shownArea(page);
    const core0 = (await bridge<any>("calc", { project: await loadProject(id) })).totals.roof_area;
    await mcp.call("update_edges", { id, changes: [{ index: 0, pitch_deg: 20 }], summary: "probe" });
    await waitForProject(id, (p) => p.parts[0].edges[0].pitch_deg === 20);
    // The review opens the AI tab; the figures are in the Results tab.
    await expect(page.locator(".review")).toBeVisible();
    await page.getByRole("tab", { name: cs("dock.results") }).click();
    const core1 = (await bridge<any>("calc", { project: await loadProject(id) })).totals.roof_area;
    expect(Math.abs(core1 - core0), "the AI change changes the area").toBeGreaterThan(0.01);
    expect(a0).toBeCloseTo(core0, 2);
    await matchesCore(page, id, "after AI save");
    await expect.poll(() => lines(page)).not.toBe(before);
  });

  test("a merge on save shows the merged roof", async ({ app, page, mcp }) => {
    const id = await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    await firstArea(page);
    await selectWall(page, 0);
    await setField(bar(page, "def.title").getByLabel(cs("edge.overhang")), 0.9);
    // Before the autosave: the save finds the AI edit on disk and merges.
    await mcp.call("update_edges", { id, changes: [{ index: 2, pitch_deg: 20 }], summary: "probe" });
    await waitForProject(id, (p) => p.parts[0].edges[0].overhang === 0.9 && p.parts[0].edges[2].pitch_deg === 20);
    await expect(page.locator(".review")).toBeVisible();
    await page.getByRole("tab", { name: cs("dock.results") }).click();
    await matchesCore(page, id, "after merge");
  });

  test("standard roof added beside the first gets its roof", async ({ app, page }) => {
    const id = await createExample("gable", "Sedlo");
    await openEditor(app, "Sedlo");
    await firstArea(page);
    const n = await page.locator(".roofline").count();
    await tool(page, "tool.standard").click();
    const dlg = page.getByRole("dialog", { name: cs("std.title") });
    await dlg.getByRole("radio", { name: cs("std.hip"), exact: true }).click();
    await dlg.getByRole("button", { name: cs("common.ok") }).click();
    await waitForProject(id, (p) => p.parts.length === 2);
    await matchesCore(page, id, "after standard roof");
    await expect.poll(() => page.locator(".roofline").count()).toBeGreaterThan(n + 3);
  });

  test("a manual face made a building gets its roof at once", async ({ app, page }) => {
    const id = await createExample("gable", "Sedlo");
    await openEditor(app, "Sedlo");
    await firstArea(page);
    await tool(page, "tool.face").click();
    for (const [x, y] of [[9, 1], [12.5, 1], [12.5, 7], [9, 7], [9, 1]]) await pressPlan(page, x, y);
    await waitForProject(id, (p) => p.manual_faces?.length === 1);
    const n = await page.locator(".roofline").count();
    await bar(page, "manual.face").getByRole("button", { name: cs("manual.toPart") }).click();
    await waitForProject(id, (p) => p.parts.length === 2);
    await matchesCore(page, id, "after make building");
    await expect.poll(() => page.locator(".roofline").count()).toBeGreaterThan(n + 2);
  });

  test("moving a part moves its roof lines", async ({ app, page }) => {
    const id = await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    await firstArea(page);
    const before = await lines(page);
    await selectWall(page, 0);
    await tool(page, "tool.transform").click();
    const dlg = page.getByRole("dialog", { name: cs("xf.title") });
    await setField(dlg.getByLabel(cs("xf.dx")), 2);
    await dlg.getByRole("button", { name: cs("common.ok") }).click();
    await waitForProject(id, (p) => p.parts[0].edges[0].start.x === 2);
    await matchesCore(page, id, "after move");
    await expect.poll(() => lines(page)).not.toBe(before);
  });
});

test.describe("every edit recalculates: parts, objects, options, revert and helpers (TEST-007)", () => {
  test("rotating a part moves its roof lines (the area stays)", async ({ app, page }) => {
    const id = await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    await firstArea(page);
    const before = await lines(page);
    await selectWall(page, 0);
    await tool(page, "tool.transform").click();
    const dlg = page.getByRole("dialog", { name: cs("xf.title") });
    await dlg.getByRole("radio", { name: cs("xf.rotate") }).click();
    await setField(dlg.getByLabel(cs("xf.angle")), 30);
    await dlg.getByRole("button", { name: cs("common.ok") }).click();
    await waitForProject(id, (p) => Math.abs(p.parts[0].edges[1].start.y) > 0.5);
    await matchesCore(page, id, "after rotate");
    await expect.poll(() => lines(page)).not.toBe(before);
  });

  test("a mirrored copy gets its roof at once", async ({ app, page }) => {
    const id = await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    const a0 = await firstArea(page);
    const n = await page.locator(".roofline").count();
    await selectWall(page, 0);
    await tool(page, "tool.transform").click();
    const dlg = page.getByRole("dialog", { name: cs("xf.title") });
    await dlg.getByRole("radio", { name: cs("xf.mirror") }).click();
    await setField(dlg.getByLabel(cs("xf.axis")), 90);
    await setField(dlg.getByLabel(cs("xf.cx")), 20);
    await setField(dlg.getByLabel(cs("xf.copies")), 1);
    await dlg.getByRole("button", { name: cs("common.ok") }).click();
    await waitForProject(id, (p) => p.parts.length === 2);
    await matchesCore(page, id, "after copy");
    await expect.poll(() => shownArea(page)).toBeGreaterThan(a0 * 1.9);
    await expect.poll(() => page.locator(".roofline").count()).toBeGreaterThanOrEqual(2 * n);
  });

  test("a window, a chimney and a dormer each recalculate the roof", async ({ app, page }) => {
    const id = await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    const a0 = await firstArea(page);
    await tool(page, "tool.window").click();
    await clickPlan(page, 3, 2);
    await waitForProject(id, (p) => p.windows?.length === 1);
    await matchesCore(page, id, "after window");
    expect(await shownArea(page)).toBeLessThan(a0);
    await page.keyboard.press("Escape");
    const a1 = await shownArea(page);
    await tool(page, "tool.chimney").click();
    await clickPlan(page, 7, 5);
    await waitForProject(id, (p) => p.chimneys.length === 1);
    await matchesCore(page, id, "after chimney");
    expect(await shownArea(page)).toBeLessThan(a1);
    await page.keyboard.press("Escape");
    const before = await lines(page);
    await tool(page, "tool.dormer").click();
    await clickPlan(page, 4, 0.5);
    await confirmDormer(page);
    await waitForProject(id, (p) => p.dormers.length === 1);
    await matchesCore(page, id, "after dormer");
    await expect.poll(() => lines(page)).not.toBe(before);
  });

  test("flat roof on and off: lines and area follow both ways", async ({ app, page }) => {
    const id = await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    const a0 = await firstArea(page);
    const l0 = await lines(page);
    await tool(page, "tool.flat").click();
    await waitForProject(id, (p) => p.flat_roof === true);
    await matchesCore(page, id, "flat on");
    await expect.poll(() => lines(page)).not.toBe(l0);
    await tool(page, "tool.flat").click();
    await waitForProject(id, (p) => !p.flat_roof);
    await matchesCore(page, id, "flat off");
    await expect.poll(() => shownArea(page)).toBeCloseTo(a0, 2);
    await expect.poll(() => lines(page)).toBe(l0);
  });

  test("deleting a part takes its roof away at once", async ({ app, page }) => {
    const id = await createExample("hip", "Valba");
    const p0 = await loadProject(id);
    const copy = structuredClone(p0.parts[0]);
    copy.name = "Garáž";
    for (const e of copy.edges) e.start.x += 20;
    p0.parts.push(copy);
    await bridge("save_project", { id, project: p0 });
    await openEditor(app, "Valba");
    const a0 = await firstArea(page);
    const n = await page.locator(".roofline").count();
    await selectWall(page, 4, 1);
    await app.answerConfirm(true);
    await tool(page, "tool.deletePart").click();
    await waitForProject(id, (p) => p.parts.length === 1);
    await matchesCore(page, id, "after delete part");
    expect(await shownArea(page)).toBeLessThan(a0 * 0.6);
    await expect.poll(() => page.locator(".roofline").count()).toBeLessThan(n);
  });

  test("Revert of an AI change in the banner shows the old roof", async ({ app, page, mcp }) => {
    const id = await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    const a0 = await firstArea(page);
    const l0 = await lines(page);
    await mcp.call("update_edges", { id, changes: [{ all: true, pitch_deg: 20 }], summary: "Sklon 20" });
    const review = page.locator(".review");
    await expect(review).toBeVisible();
    await review.getByRole("button", { name: cs("ai.revert") }).click();
    await waitForProject(id, (p) => p.parts[0].edges[0].pitch_deg !== 20);
    await page.getByRole("tab", { name: cs("dock.results") }).click();
    await matchesCore(page, id, "after revert");
    await expect.poll(() => shownArea(page)).toBeCloseTo(a0, 2);
    await expect.poll(() => lines(page)).toBe(l0);
  });

  test("the round roof helper: the roof follows its pitches", async ({ app, page }) => {
    const id = await createExample("gable", "Sedlo");
    await openEditor(app, "Sedlo");
    await firstArea(page);
    const l0 = await lines(page);
    await selectWall(page, 0);
    await bar(page, "def.title").getByRole("button", { name: cs("round.button") }).click();
    await page.getByRole("dialog", { name: cs("round.title") }).getByRole("button", { name: cs("common.ok") }).click();
    await waitForProject(id, (p) => (p.parts[0].edges[0].breaks?.length ?? 0) > 0);
    await matchesCore(page, id, "after round roof");
    await expect.poll(() => lines(page)).not.toBe(l0);
  });

  test("a calculation helper: the roof follows the build-up it sets", async ({ app, page }) => {
    const id = await createExample("gable", "Sedlo");
    await openEditor(app, "Sedlo");
    await firstArea(page);
    const l0 = await lines(page);
    const e0 = (await loadProject(id)).parts[0].edges[0];
    await selectWall(page, 0);
    await bar(page, "def.title").getByRole("button", { name: cs("calc.button") }).click();
    const dlg = page.getByRole("dialog", { name: cs("calc.title") });
    await dlg.getByRole("tab", { name: cs("calc.buildup") }).click();
    await setField(dlg.getByRole("textbox", { name: cs("calc.thickness") }), "0,3");
    await dlg.getByRole("button", { name: cs("common.ok") }).click();
    await waitForProject(id, (p) => p.parts[0].edges[0].overhang !== e0.overhang);
    await matchesCore(page, id, "after build-up");
    await expect.poll(() => lines(page)).not.toBe(l0);
  });
});
