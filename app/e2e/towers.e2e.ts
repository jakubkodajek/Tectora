// Towers and curved walls (ADR 0008).
//
// Edge-cases class 2 for the new operations (review TEST-004, moved here
// from the review board's spec): after a tower placed with the tool, its
// undo, redo, edit and delete, an AI set_tower, an apse, a bent wall and a
// rounded corner, the shown area equals the core's result of the saved file
// and the roof lines follow. MCP parity of the tower dialog and set_tower.
// Then the guards of the review's dialog findings: a refused typed value
// keeps the dialog open (ERR-003), arrows in an object's menu leave it where
// it is (UI-001), a double click on "remove tier" removes one (UI-004), and
// a tower the core does not calculate can be selected (UI-002).
import type { Page } from "@playwright/test";
import { bridge, createExample, expect, loadProject, test, waitForProject, type App } from "./fixtures";
import { firstArea, planXY, shownArea, tool, openEditor } from "./editor";
import { cs } from "./i18n";

const job = (example: string, name: string) => createExample(example, name);
const open = (app: App, name: string) => openEditor(app, name);
const core = async (id: string) => await bridge<any>("calc", { project: await loadProject(id) });
async function matchesCore(page: Page, id: string, what: string) {
  await expect.poll(async () => Math.abs((await shownArea(page)) - (await core(id)).totals.roof_area), { message: what, timeout: 10000 }).toBeLessThan(0.01);
}
// The status bar's roof area is on screen whatever dock tab is open (an AI save reopens the AI tab).
async function barArea(page: Page): Promise<number> {
  const text = (await page.locator(".statusbar").textContent()) ?? "";
  const m = text.match(/Plocha střechy:\s*([\d\s,.]+)\s*m²/);
  return m ? Number(m[1].replace(/\s/g, "").replace(",", ".")) : NaN;
}
async function barMatchesCore(page: Page, id: string, what: string) {
  await expect.poll(async () => Math.abs((await barArea(page)) - (await core(id)).totals.roof_area), { message: what, timeout: 10000 }).toBeLessThan(0.01);
}
const lines = (page: Page) => page.locator(".roofline").evaluateAll((ls) => ls.map((l) => [l.getAttribute("x1"), l.getAttribute("y1"), l.getAttribute("x2"), l.getAttribute("y2")].join(",")).sort().join(";"));

test("C2-a: a tower placed with the tool, undone, redone and deleted", async ({ app, page }) => {
  const id = await job("hip", "E2E vez");
  await open(app, "E2E vez");
  const a0 = await firstArea(page);
  await tool(page, "tool.tower").click();
  const at = await planXY(page, 6, 4);
  await page.mouse.click(at.x, at.y);
  const dlg = page.getByRole("dialog", { name: cs("tower.dialogNew") });
  await expect(dlg).toBeVisible();
  await page.getByTestId("tower-ok").click();
  await expect(dlg).toBeHidden();
  await waitForProject(id, (p) => (p.towers ?? []).length === 1);
  await matchesCore(page, id, "after tower add");
  const a1 = await shownArea(page);
  expect(Math.abs(a1 - a0), "the tower changes the area").toBeGreaterThan(0.01);
  await tool(page, "editor.undo").click();
  await waitForProject(id, (p) => (p.towers ?? []).length === 0);
  await matchesCore(page, id, "after undo");
  expect(await shownArea(page)).toBeCloseTo(a0, 2);
  await tool(page, "editor.redo").click();
  await waitForProject(id, (p) => (p.towers ?? []).length === 1);
  await matchesCore(page, id, "after redo");
  expect(await shownArea(page)).toBeCloseTo(a1, 2);
  // Select the tower in the plan, edit its width in the dialog.
  const c = await planXY(page, 6, 4);
  await page.mouse.click(c.x, c.y);
  const tbar = page.getByTestId("tower-bar");
  await expect(tbar).toBeVisible();
  await tbar.getByRole("button", { name: cs("tower.edit") }).click();
  const dlg2 = page.getByRole("dialog");
  const width = dlg2.getByLabel(cs("tower.width")).first();
  await width.fill("4");
  await width.press("Tab");
  await page.getByTestId("tower-ok").click();
  await waitForProject(id, (p) => p.towers?.[0]?.width === 4);
  await matchesCore(page, id, "after tower edit");
  expect(Math.abs((await shownArea(page)) - a1)).toBeGreaterThan(0.01);
  // Delete with the bar's button.
  await expect(tbar).toBeVisible();
  await tbar.getByRole("button", { name: cs("tower.delete") }).click();
  await waitForProject(id, (p) => (p.towers ?? []).length === 0);
  await matchesCore(page, id, "after tower delete");
  expect(await shownArea(page)).toBeCloseTo(a0, 2);
});

test("C2-b: an AI set_tower save and its change shows the new roof", async ({ app, page, mcp }) => {
  const id = await job("hip", "E2E vez AI");
  await open(app, "E2E vez AI");
  await firstArea(page);
  const a0 = await barArea(page);
  {
    const r = await (mcp as any).tryCall("set_tower", { id, summary: "probe", preset: "onion", center: [6, 4], width: 2, base_height: 12 });
    expect(r.isError, r.text).toBe(false);
    await waitForProject(id, (p) => (p.towers ?? []).length === 1);
    await expect(page.locator(".review")).toBeVisible({ timeout: 15000 });
    await page.getByRole("tab", { name: cs("dock.results") }).click();
    await barMatchesCore(page, id, "after AI set_tower");
    expect(Math.abs((await barArea(page)) - a0)).toBeGreaterThan(0.01);
    const a1 = await barArea(page);
    const r2 = await (mcp as any).tryCall("set_tower", { id, summary: "probe 2", index: 0, width: 3 });
    expect(r2.isError, r2.text).toBe(false);
    await waitForProject(id, (p) => p.towers?.[0]?.width === 3);
    await page.getByRole("tab", { name: cs("dock.results") }).click();
    await barMatchesCore(page, id, "after AI tower change");
    expect(Math.abs((await barArea(page)) - a1)).toBeGreaterThan(0.01);
    const r3 = await (mcp as any).tryCall("delete_items", { id, summary: "probe 3", items: [{ kind: "tower", index: 0 }] });
    expect(r3.isError, r3.text).toBe(false);
    await waitForProject(id, (p) => (p.towers ?? []).length === 0);
    await page.getByRole("tab", { name: cs("dock.results") }).click();
    await barMatchesCore(page, id, "after AI tower delete");
    expect(await barArea(page)).toBeCloseTo(a0, 2);
  }
});

test("C2-c: an apse, a bent wall and a rounded corner from the wall menu", async ({ app, page }) => {
  const id = await job("hip", "E2E oblouk");
  await open(app, "E2E oblouk");
  await firstArea(page);
  const p0 = await loadProject(id);
  const n0 = p0.parts[0].edges.length;
  const l0 = await lines(page);
  const hit = (i: number) => page.locator(".view .wrap > svg g.edge line.hit").nth(i);
  // Apse on wall 2 (index 1).
  await hit(1).click({ button: "right", force: true });
  await page.getByRole("menuitem", { name: cs("arc.apse") }).click();
  const p1 = await waitForProject(id, (p) => p.parts[0].edges.length > n0);
  await matchesCore(page, id, "after apse");
  await expect.poll(() => lines(page)).not.toBe(l0);
  const l1 = await lines(page);
  // Undo, redo.
  await tool(page, "editor.undo").click();
  await waitForProject(id, (p) => p.parts[0].edges.length === n0);
  await matchesCore(page, id, "after undo of apse");
  await expect.poll(() => lines(page)).toBe(l0);
  await tool(page, "editor.redo").click();
  await waitForProject(id, (p) => p.parts[0].edges.length === p1.parts[0].edges.length);
  await matchesCore(page, id, "after redo of apse");
  await expect.poll(() => lines(page)).toBe(l1);
  // Bend wall 1 (index 0) through the dialog.
  await hit(0).click({ button: "right", force: true });
  await page.getByRole("menuitem", { name: cs("arc.bend") }).click();
  await expect(page.getByTestId("arc-ok")).toBeVisible();
  await page.getByTestId("arc-ok").click();
  const n1 = p1.parts[0].edges.length;
  await waitForProject(id, (p) => p.parts[0].edges.length > n1);
  await matchesCore(page, id, "after bend");
  await expect.poll(() => lines(page)).not.toBe(l1);
});

test("C2-d: a rounded corner from the corner menu", async ({ app, page }) => {
  const id = await job("gable", "E2E roh");
  await open(app, "E2E roh");
  await firstArea(page);
  const p0 = await loadProject(id);
  const n0 = p0.parts[0].edges.length;
  const v = p0.parts[0].edges[2].start;
  const l0 = await lines(page);
  const at = await planXY(page, v.x, v.y);
  await page.mouse.click(at.x, at.y, { button: "right" });
  await page.getByRole("menuitem", { name: cs("arc.round") }).click();
  await expect(page.getByTestId("arc-ok")).toBeVisible();
  await page.getByTestId("arc-ok").click();
  await waitForProject(id, (p) => p.parts[0].edges.length > n0);
  await matchesCore(page, id, "after round corner");
  await expect.poll(() => lines(page)).not.toBe(l0);
});

test("PAR-a: a tower from the tool and the same tower over MCP give the same file and result", async ({ app, page, mcp }) => {
  const a = await job("hip", "E2E par UI");
  const b = await job("hip", "E2E par MCP");
  await open(app, "E2E par UI");
  await firstArea(page);
  await tool(page, "tool.tower").click();
  const at = await planXY(page, 6, 4);
  await page.mouse.click(at.x, at.y);
  await page.getByTestId("tower-preset-baroque").click();
  await expect.poll(async () => page.getByRole("dialog").locator("input").count()).toBeGreaterThan(20);
  await page.getByTestId("tower-ok").click();
  const pa = await waitForProject(a, (p) => (p.towers ?? []).length === 1);
  const t = pa.towers[0];
  const r = await (mcp as any).tryCall("set_tower", { id: b, summary: "parity", preset: "baroque", center: [t.center.x, t.center.y], width: t.width, base_height: t.base_height });
  expect(r.isError, r.text).toBe(false);
  const pb = await waitForProject(b, (p) => (p.towers ?? []).length === 1);
  expect(pb.towers).toEqual(pa.towers);
  expect((await core(b)).totals).toEqual((await core(a)).totals);
});

test("ERR-003: a width typed out of range keeps the tower dialog open with the reason, and stores nothing", async ({ app, page }) => {
  const id = await job("hip", "E2E vez typed");
  await open(app, "E2E vez typed");
  await firstArea(page);
  await tool(page, "tool.tower").click();
  const at = await planXY(page, 6, 4);
  await page.mouse.click(at.x, at.y);
  const dlg = page.getByRole("dialog", { name: cs("tower.dialogNew") });
  await expect(dlg).toBeVisible();
  const width = dlg.getByLabel(cs("tower.width")).first();
  await width.fill("500");
  await expect(width).toHaveAttribute("aria-invalid", "true");
  // The real mouse on OK, the focus still in the field (as in WebKit).
  const ok = page.getByTestId("tower-ok");
  const box = (await ok.boundingBox())!;
  await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
  await expect(page.getByTestId("tower-ok-blocked")).toContainText(cs("tower.width"));
  await expect(dlg).toBeVisible();
  expect((await loadProject(id)).towers ?? []).toHaveLength(0);
  // A width it takes: OK stores that one.
  await width.fill("2,5");
  await expect(page.getByTestId("tower-ok-blocked")).toBeHidden();
  await ok.click();
  const p = await waitForProject(id, (x) => (x.towers ?? []).length === 1);
  expect(p.towers[0].width).toBe(2.5);
});

test("UI-001: arrow keys in a tower's menu walk the menu and leave the tower where it is", async ({ app, page, mcp }) => {
  const id = await job("hip", "E2E vez menu");
  await open(app, "E2E vez menu");
  await firstArea(page);
  const r = await (mcp as any).tryCall("set_tower", { id, summary: "a turret for the menu test", preset: "pyramid", center: [6, 4], width: 2, base_height: 12 });
  expect(r.isError, r.text).toBe(false);
  const before = (await waitForProject(id, (p) => (p.towers ?? []).length === 1)).towers[0].center;
  // The editor took the AI's save in: the tower is in the plan.
  await expect(page.getByTestId("plan-tower")).toHaveCount(1);
  await barMatchesCore(page, id, "after the AI's tower");
  const at = await planXY(page, 6, 4);
  await page.mouse.click(at.x, at.y, { button: "right" });
  const menu = page.getByRole("menu");
  await expect(menu).toBeVisible();
  for (let i = 0; i < 3; i++) await page.keyboard.press("ArrowDown");
  await page.keyboard.press("ArrowLeft");
  await page.keyboard.press("Escape");
  await expect(menu).toBeHidden();
  await barMatchesCore(page, id, "after the menu closed");
  expect((await loadProject(id)).towers[0].center).toEqual(before);
});

test("UI-004 / UI-003: a double click on remove tier removes one tier, and the focus stays in the table", async ({ app, page }) => {
  await job("hip", "E2E vez pasy");
  await open(app, "E2E vez pasy");
  await firstArea(page);
  await tool(page, "tool.tower").click();
  const at = await planXY(page, 6, 4);
  await page.mouse.click(at.x, at.y);
  const dlg = page.getByRole("dialog", { name: cs("tower.dialogNew") });
  await page.getByTestId("tower-preset-baroque").click();
  const rows = dlg.getByTestId("tower-tier");
  await expect(rows).toHaveCount(7);
  await dlg.getByRole("button", { name: cs("tower.removeTier", { n: 4 }) }).dblclick();
  await expect(rows).toHaveCount(6);
  // Moving a tier by keyboard keeps the focus on a button of the table.
  await dlg.getByRole("button", { name: cs("tower.tierUp", { n: 2 }) }).focus();
  await page.keyboard.press("Enter");
  await expect.poll(() => page.evaluate(() => document.activeElement?.closest("[data-testid=tower-tier]")?.getAttribute("data-tier"))).toBe("2");
});

test("UI-002: a tower the core does not calculate can be selected and deleted in the plan", async ({ app, page }) => {
  const id = await job("hip", "E2E vez chybna");
  const p = await loadProject(id);
  p.towers = [{ label: "", center: { x: 6, y: 4 }, width: 2, base_height: 12, tiers: [{ rise: 1, top_width: 0, bulge: 5 }] }];
  await bridge("save_project", { id, project: p });
  await open(app, "E2E vez chybna");
  await firstArea(page);
  const at = await planXY(page, 6.3, 4.3);
  await page.mouse.click(at.x, at.y);
  const tbar = page.getByTestId("tower-bar");
  await expect(tbar).toBeVisible();
  await tbar.getByRole("button", { name: cs("tower.delete") }).click();
  await waitForProject(id, (x) => (x.towers ?? []).length === 0);
  await matchesCore(page, id, "after the bad tower was deleted");
});
