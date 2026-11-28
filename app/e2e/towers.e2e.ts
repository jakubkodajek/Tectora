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
import { bridge, createExample, expect, loadProject, noToast, test, waitForProject, type App } from "./fixtures";
import { firstArea, planXY, selectWall, shownArea, tool, openEditor } from "./editor";
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

// ---- Round 2 of the towers audit -----------------------------------------

/** Opens the new-tower dialog at (6, 4) of the open hip job. */
async function newTowerDialog(page: Page) {
  await tool(page, "tool.tower").click();
  const at = await planXY(page, 6, 4);
  await page.mouse.click(at.x, at.y);
  const dlg = page.getByRole("dialog", { name: cs("tower.dialogNew") });
  await expect(dlg).toBeVisible();
  return dlg;
}

/** A press beside every field, on the dialog's own preview caption (a real mouse press, as the user leaves a field). */
async function pressBeside(page: Page) {
  const box = (await page.getByRole("dialog").locator("figcaption").boundingBox())!;
  await page.mouse.click(box.x + 4, box.y + box.height / 2);
}

test("ERR-001: 3000 typed key by key for a 3 m width leaves 3 m, not 30 m, and OK stores 3 m", async ({ app, page }) => {
  const id = await job("hip", "E2E vez 3000");
  await open(app, "E2E vez 3000");
  await firstArea(page);
  const dlg = await newTowerDialog(page);
  const width = dlg.getByLabel(cs("tower.width")).first();
  await expect(width).toHaveValue("3");
  await width.click();
  await width.press("Control+a");
  await page.keyboard.type("3000");
  await expect(width).toHaveAttribute("aria-invalid", "true");
  // While the text is refused, OK is blocked with the reason.
  await expect(page.getByTestId("tower-ok")).toBeDisabled();
  await expect(page.getByTestId("tower-ok-blocked")).toContainText(cs("tower.width"));
  await pressBeside(page);
  // The field shows the value from before the typing, not the prefix "30".
  await expect(width).toHaveValue("3");
  await expect(page.getByTestId("tower-ok-blocked")).toBeHidden({ timeout: 6000 });
  await page.getByTestId("tower-ok").click();
  const p = await waitForProject(id, (x) => (x.towers ?? []).length === 1);
  expect(p.towers[0].width).toBe(3);
});

test("ERR-001: Escape after 250 typed into a tier's rise puts back the rise from before the typing", async ({ app, page }) => {
  const id = await job("hip", "E2E vez esc");
  await open(app, "E2E vez esc");
  await firstArea(page);
  const dlg = await newTowerDialog(page);
  const rise = dlg.getByLabel(`${cs("tower.tierN", { n: 1 })}: ${cs("tower.rise")}`);
  const before = await rise.inputValue();
  await rise.click();
  await rise.press("Control+a");
  await page.keyboard.type("250");
  await expect(rise).toHaveAttribute("aria-invalid", "true");
  await rise.press("Escape");
  await expect(rise).toHaveValue(before);
  await expect(dlg).toBeVisible();
  await page.getByTestId("tower-ok").click();
  const p = await waitForProject(id, (x) => (x.towers ?? []).length === 1);
  expect(String(p.towers[0].tiers[0].rise).replace(".", ",")).toBe(before);
});

test("ERR-001: 100 walls typed for an arc (at most 64) makes no 10-wall arc", async ({ app, page }) => {
  const id = await job("hip", "E2E oblouk 100");
  await open(app, "E2E oblouk 100");
  await firstArea(page);
  const n0 = (await loadProject(id)).parts[0].edges.length;
  await page.locator(".view .wrap > svg g.edge line.hit").nth(0).click({ button: "right", force: true });
  await page.getByRole("menuitem", { name: cs("arc.bend") }).click();
  const seg = page.getByRole("dialog").getByPlaceholder(cs("arc.segmentsAuto"));
  await seg.click();
  await page.keyboard.type("100");
  await expect(seg).toHaveAttribute("aria-invalid", "true");
  await pressBeside2(page);
  // Back to "automatic" (empty), as before the typing.
  await expect(seg).toHaveValue("");
  await expect(seg).not.toHaveAttribute("aria-invalid", "true", { timeout: 6000 });
  await page.getByTestId("arc-ok").click();
  const p = await waitForProject(id, (x) => x.parts[0].edges.length > n0);
  expect(p.parts[0].edges.length - n0 + 1).not.toBe(10);
});
async function pressBeside2(page: Page) {
  const box = (await page.getByRole("dialog").locator(".hint").first().boundingBox())!;
  await page.mouse.click(box.x + 2, box.y + box.height / 2);
}

test("ERR-004: a preset the tower limits refuse is a note in the dialog, not a failed-calculation toast", async ({ app, page }) => {
  await job("hip", "E2E vez predloha");
  await open(app, "E2E vez predloha");
  await firstArea(page);
  const dlg = await newTowerDialog(page);
  const width = dlg.getByLabel(cs("tower.width")).first();
  await width.fill("70");
  await width.press("Tab");
  await page.getByTestId("tower-preset-octagon").click();
  await expect(page.getByTestId("tower-preset-refused")).toBeVisible();
  await noToast(app.toasts().filter({ hasText: cs("err.action.calc") }), 1500, "a failed-calculation toast");
  await expect(dlg).toBeVisible();
});

test("UX-004: a double click on the roof bar's semicircular end makes one apse and no error note", async ({ app, page }) => {
  const id = await job("hip", "E2E apsida dvojklik");
  await open(app, "E2E apsida dvojklik");
  await firstArea(page);
  const n0 = (await loadProject(id)).parts[0].edges.length;
  await selectWall(page, 1);
  const btn = page.getByTestId("def-arcs").getByRole("button", { name: cs("arc.apse") });
  await btn.scrollIntoViewIfNeeded();
  const box = (await btn.boundingBox())!;
  await page.mouse.dblclick(box.x + box.width / 2, box.y + box.height / 2);
  const p = await waitForProject(id, (x) => x.parts[0].edges.length > n0);
  await noToast(app.toasts(), 2000, "a toast after the double click");
  await matchesCore(page, id, "after the apse");
  expect((await loadProject(id)).parts[0].edges.length).toBe(p.parts[0].edges.length);
});

test("UX-003: deleting a building names its tower in the question and deletes the tower with it", async ({ app, page, mcp }) => {
  const id = await job("hip", "E2E vez s budovou");
  // A second building beside, so the delete is not Clear.
  {
    const p = await loadProject(id);
    const other = JSON.parse(JSON.stringify(p.parts[0]));
    for (const e of other.edges) e.start.x += 30;
    other.name = "Budova 2";
    p.parts.push(other);
    await bridge("save_project", { id, project: p });
  }
  await open(app, "E2E vez s budovou");
  await firstArea(page);
  const r = await (mcp as any).tryCall("set_tower", { id, summary: "a turret on building 1", preset: "pyramid", center: [6, 4], width: 2, base_height: 12, label: "Sanktusník" });
  expect(r.isError, r.text).toBe(false);
  await waitForProject(id, (p) => (p.towers ?? []).length === 1);
  await expect(page.getByTestId("plan-tower")).toHaveCount(1);
  await barMatchesCore(page, id, "after the AI's tower");
  await selectWall(page, 0);
  await app.answerConfirm(true);
  await page.getByRole("button", { name: cs("tool.deletePart") }).first().click();
  await waitForProject(id, (p) => p.parts.length === 1);
  const asked = (await app.dialogs()).filter((d) => d.kind === "confirm").map((d) => d.message);
  expect(asked.at(-1)).toContain("Sanktusník");
  expect((await loadProject(id)).towers ?? []).toHaveLength(0);
  await barMatchesCore(page, id, "after the building and its tower were deleted");
});

test("UX-011: Cancel and Escape ask before dropping a typed name, and keep the dialog on No", async ({ app, page }) => {
  await job("hip", "E2E vez zavrit");
  await open(app, "E2E vez zavrit");
  await firstArea(page);
  const dlg = await newTowerDialog(page);
  await dlg.getByLabel(cs("tower.label")).fill("Lucerna");
  await app.answerConfirm(false);
  await dlg.getByRole("button", { name: cs("common.cancel") }).click();
  await expect.poll(async () => (await app.dialogs()).filter((d) => d.kind === "confirm").length).toBe(1);
  await expect(dlg).toBeVisible();
  await app.answerConfirm(true);
  await page.keyboard.press("Escape");
  await expect(dlg).toBeHidden();
  expect((await app.dialogs()).filter((d) => d.kind === "confirm").map((d) => d.message)).toEqual([cs("tower.confirmDiscard"), cs("tower.confirmDiscard")]);
});

test("UX-011: a dialog closed without a change asks nothing", async ({ app, page }) => {
  await job("hip", "E2E vez bez zmeny");
  await open(app, "E2E vez bez zmeny");
  await firstArea(page);
  const dlg = await newTowerDialog(page);
  await dlg.getByRole("button", { name: cs("common.cancel") }).click();
  await expect(dlg).toBeHidden();
  expect((await app.dialogs()).filter((d) => d.kind === "confirm")).toEqual([]);
});

test("UX-002 / UX-008: a tower roof below the roof is a warning on the Results tab; the breakdown lists the shaft's cladding", async ({ app, page, mcp }) => {
  const id = await job("hip", "E2E vez pod strechou");
  await open(app, "E2E vez pod strechou");
  await firstArea(page);
  const r = await (mcp as any).tryCall("set_tower", { id, summary: "a tower sunk into the roof", preset: "pyramid", center: [6, 4], width: 2, base_height: 4, label: "Nízká" });
  expect(r.isError, r.text).toBe(false);
  await waitForProject(id, (p) => (p.towers ?? []).length === 1);
  await expect(page.getByTestId("plan-tower")).toHaveCount(1);
  await barMatchesCore(page, id, "after the AI's tower");
  await page.getByRole("tab", { name: cs("dock.results") }).click();
  await expect(page.getByTestId("results-warned")).toBeVisible();
  await expect(page.getByTestId("res-tower-below")).toBeVisible();
  await expect(page.locator(".results .warn")).toContainText("Nízká");
  // A tower standing out of the roof: no warning, and its cladding is in the breakdown.
  const r2 = await (mcp as any).tryCall("set_tower", { id, summary: "raise it", index: 0, base_height: 12 });
  expect(r2.isError, r2.text).toBe(false);
  await waitForProject(id, (p) => p.towers?.[0]?.base_height === 12);
  await page.getByRole("tab", { name: cs("dock.results") }).click();
  await expect(page.getByTestId("res-tower-below")).toHaveCount(0);
  await page.getByRole("tab", { name: cs("dock.breakdown") }).click();
  await expect(page.getByTestId("bd-tower-shaft")).toBeVisible();
});
