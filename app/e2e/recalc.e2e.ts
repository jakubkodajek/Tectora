// Edge-cases class 2 (TEST-003): after every operation that changes the job
// (undo, redo, an AI save, a merge on save, a standard roof, a manual face
// made a building, a move), the shown result equals the core's calculation
// of the saved file and the plan's roof lines follow, with no further action.
import type { Page } from "@playwright/test";
import { bar, firstArea, openEditor, selectWall, setField, shownArea, tool, planXY } from "./editor";
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
