// Several buildings in one job: a second building gets its roof at once, the
// first building's gable stays intact, joined roofs say what they are joined
// to, lines under a higher roof are dashed, and a building hidden inside
// another is reported instead of silently showing no roof.
import type { Page } from "@playwright/test";
import { bar, clickPlan, openEditor, plan, planXY, selectWall, setField, shownArea, tool } from "./editor";
import { bridge, createExample, expect, loadProject, test, waitForProject } from "./fixtures";
import { cs } from "./i18n";

function rect(x0: number, y0: number, x1: number, y1: number, eave = 3, name = "P2") {
  return {
    name,
    edges: [
      [x0, y0],
      [x1, y0],
      [x1, y1],
      [x0, y1],
    ].map(([x, y]) => ({ start: { x, y }, pitch_deg: 40, eave_height: eave, overhang: 0.5 })),
  };
}

async function withSecondPart(id: string, part: object) {
  const p = await loadProject(id);
  p.parts.push(part);
  await bridge("save_project", { id, project: p });
}

/**
 * The core's roof lines of each part of the saved job, as comparable text:
 * kind and end points to 1 cm, in any order and direction. Several parts go
 * through the multi-roof join, which works in single precision and lists
 * lines in its own order, so the same roof differs by float noise.
 */
async function coreLines(id: string): Promise<string[]> {
  const r = await bridge<any>("calc", { project: await loadProject(id) });
  const pt = (p: number[]) => p.map((v) => (Math.round(v * 100) / 100 + 0).toFixed(2)).join(" ");
  return r.parts.map((rp: any) =>
    JSON.stringify(rp.geometry.lines.map((l: any) => `${l.kind} ${[pt(l.a), pt(l.b)].sort().join(" / ")}`).sort()),
  );
}

/** The results tab shows the core's total for the saved job. */
async function areaMatchesCore(page: Page, id: string) {
  await expect
    .poll(async () => {
      const r = await bridge<any>("calc", { project: await loadProject(id) });
      return Math.abs((await shownArea(page)) - r.totals.roof_area);
    })
    .toBeLessThan(0.01);
}

/** Screen boxes of the drawn wall dimensions and face labels. */
const labelBoxes = (page: Page) =>
  page.locator(".view .wrap > svg").first().locator(".edge-dim, .face-tag").evaluateAll((els) =>
    els.map((e) => {
      const r = e.getBoundingClientRect();
      return { t: e.textContent, x0: r.left, y0: r.top, x1: r.right, y1: r.bottom };
    }),
  );

/** The plan extent the 3D view is framed for: [x0, y0, x1, y1]. */
const frame3d = async (page: Page) => ((await page.locator(".host[data-frame]").getAttribute("data-frame")) ?? "").split(",").map(Number);

test.describe("several buildings", () => {
  test("a building drawn next to the first gets its roof at once and is named as joined", async ({ app, page }) => {
    const id = await createExample("gable", "Sedlo");
    await openEditor(app, "Sedlo");
    const roofLines = page.locator(".roofline");
    const before = await roofLines.count();
    await tool(page, "tool.draw").click();
    const draw = page.getByRole("toolbar", { name: cs("draw.title") });
    await clickPlan(page, 12.3, 1);
    const len = draw.getByLabel(cs("draw.length"));
    await len.fill("3");
    await len.press("ArrowRight");
    await len.fill("6");
    await len.press("ArrowUp");
    await len.fill("3");
    await len.press("ArrowLeft");
    await len.press("Enter");
    await waitForProject(id, (p) => p.parts.length === 2);
    // The new building's hips and ridge are in the plan right away.
    await expect.poll(() => roofLines.count()).toBeGreaterThan(before + 3);
    const results = page.locator(".results");
    // The template's untouched "Main" is shown as the localized "Budova 1" (UX-010).
    expect((await loadProject(id)).parts[0].name).toBe("Main");
    await expect(results).toContainText(cs("res.joinedTo", { names: cs("editor.partN", { n: 1 }) }));
    // The second roof's stretches under the main roof's overhang are dashed.
    await expect(page.locator(".roofline.covered").first()).toBeAttached();
    // The results are the core's for the saved job.
    await areaMatchesCore(page, id);
  });

  test("buildings whose eaves only come close are not named as joined", async ({ app, page }) => {
    // ARCH-008: 0.8 m apart with 0.5 m overhangs; the calculation leaves
    // both roofs as they are alone, so the panel does not say "joined".
    const id = await createExample("gable", "Sedlo");
    const p = await loadProject(id);
    const xs = p.parts[0].edges.map((e: any) => e.start.x);
    const ys = p.parts[0].edges.map((e: any) => e.start.y);
    const x = Math.max(...xs) + 0.8;
    await withSecondPart(id, rect(x, Math.min(...ys) + 1, x + 4, Math.max(...ys) - 1));
    await openEditor(app, "Sedlo");
    const results = page.locator(".results");
    await expect(results.locator("table").first()).toContainText("P2");
    await expect(results).not.toContainText(cs("res.joinedTo", { names: p.parts[0].name }));
    const r = await bridge<any>("calc", { project: await loadProject(id) });
    expect(r.parts.map((q: any) => q.geometry.joined_with ?? [])).toEqual([[], []]);
  });

  test("a dormer beside the roof is reported and not counted", async ({ app, page }) => {
    // ERR-011: it used to switch the job to the fallback engine in silence.
    const id = await createExample("gable", "Sedlo");
    const before = await bridge<any>("calc", { project: await loadProject(id) });
    const p = await loadProject(id);
    p.dormers.push({
      edges: [
        { start: { x: 40, y: 40 }, pitch_deg: 45, eave_height: 4, overhang: 0, direction_rad: 4.71238898038469 },
        { start: { x: 40, y: 40 }, pitch_deg: 90, eave_height: 4, overhang: 0 },
        { start: { x: 42, y: 40 }, pitch_deg: 45, eave_height: 4, overhang: 0, direction_rad: 1.5707963267948966 },
      ],
      end: { x: 42, y: 40 },
    });
    await bridge("save_project", { id, project: p });
    await openEditor(app, "Sedlo");
    await expect(page.locator(".results")).toContainText(cs("res.warn.dormer_off_roof", { n: 1 }));
    const after = await bridge<any>("calc", { project: await loadProject(id) });
    expect(after.totals.roof_area).toBe(before.totals.roof_area);
  });

  test("a building inside another is reported, not silently roofless", async ({ app, page }) => {
    const id = await createExample("gable", "Sedlo");
    await withSecondPart(id, rect(4, 2, 7, 5));
    await openEditor(app, "Sedlo");
    await expect(page.locator(".results")).toContainText(cs("res.warn.covered"));
    await areaMatchesCore(page, id);
  });

  test("changing the second building's eave height recalculates both roofs", async ({ app, page }) => {
    const id = await createExample("gable", "Sedlo");
    await withSecondPart(id, rect(11, 1, 14, 7));
    await openEditor(app, "Sedlo");
    const area = page.locator(".results .hero strong");
    const def = bar(page, "def.title");
    await selectWall(page, 4, 1);
    const lines = async () => (await page.locator(".roofline").evaluateAll((ls) => ls.map((l) => l.getAttribute("x1") + "," + l.getAttribute("y1")).join(";")));
    const shape = await lines();
    const core = await coreLines(id);
    await setField(def.getByLabel(cs("edge.eaveHeight")), "4,5");
    await waitForProject(id, (p) => p.parts[1].edges[0].eave_height === 4.5);
    await expect.poll(lines).not.toBe(shape);
    await expect(area).not.toHaveText("");
    // Both roofs are recalculated, not only the edited one (TEST-005):
    // the main roof's lines where the wing meets it change too.
    const after = await coreLines(id);
    expect(after[1]).not.toBe(core[1]);
    expect(after[0]).not.toBe(core[0]);
    await areaMatchesCore(page, id);
  });

  test("the building height moves every wall of the building in one change and rejoins both roofs", async ({ app, page }) => {
    // Jakub 2026-11-08: two hip roofs side by side, the second lower, roofs
    // overlapping; the building's height was only settable wall by wall.
    // Jakub 2026-11-14: the building height is not the eave height; a wall
    // whose eave differs keeps its offset.
    const id = await createExample("hip", "Valba");
    const wing = rect(10, 2, 16, 7, 3, "Křídlo");
    wing.edges[2].eave_height = 3.4;
    await withSecondPart(id, wing);
    await openEditor(app, "Valba");
    const def = bar(page, "def.title");
    await selectWall(page, 4, 1);
    const field = def.getByLabel(cs("edge.buildingHeight"));
    // A file without a building height shows its lowest eave.
    await expect(field).toHaveValue("3");
    const main = (await loadProject(id)).parts[0];
    const core = await coreLines(id);
    await setField(field, "1,8");
    const saved = await waitForProject(id, (p) => p.parts[1].height === 1.8);
    expect(saved.parts[1].edges.map((e: any) => e.eave_height)).toEqual([1.8, 1.8, 2.2, 1.8]);
    expect(saved.parts[0]).toEqual(main);
    await expect(field).toHaveValue("1,8");
    // Both roofs are recalculated: the main roof's lines where the wing meets it change too.
    const after = await coreLines(id);
    expect(after[1]).not.toBe(core[1]);
    expect(after[0]).not.toBe(core[0]);
    await areaMatchesCore(page, id);
    // One change: one undo brings every wall back.
    await tool(page, "editor.undo").click();
    await waitForProject(id, (p) => p.parts[1].edges[2].eave_height === 3.4 && p.parts[1].edges[0].eave_height === 3);
  });

  test("a building far from the first leaves the first roof as it was and is shown in 3D", async ({ app, page }) => {
    const id = await createExample("gable", "Sedlo");
    await openEditor(app, "Sedlo");
    await expect.poll(() => frame3d(page)).toHaveLength(4);
    const first = await coreLines(id);
    const [, , x1] = await frame3d(page);
    expect(x1).toBeLessThan(15);
    // A second building at x 20..26 m, added with the Standard roof dialog.
    await tool(page, "tool.standard").click();
    const dlg = page.getByRole("dialog", { name: cs("std.title") });
    await dlg.getByLabel(cs("std.add")).check();
    await expect(dlg.locator(".preview .area")).toBeVisible();
    await dlg.getByRole("button", { name: cs("common.ok"), exact: true }).click();
    const p = await waitForProject(id, (p) => p.parts.length === 2);
    const right = Math.max(...p.parts[1].edges.map((e: any) => e.start.x));
    // The first roof is untouched where the new one does not reach.
    expect((await coreLines(id))[0]).toBe(first[0]);
    await areaMatchesCore(page, id);
    // The 3D view reframes so the new roof is in view (UI-007).
    await expect.poll(async () => (await frame3d(page))[2]).toBeGreaterThanOrEqual(right - 0.01);
  });

  test("Duplicate part puts the copy beside every building, not on one (UX-010)", async ({ app, page }) => {
    const id = await createExample("gable", "Sedlo");
    await withSecondPart(id, rect(14, 1, 18, 6));
    await openEditor(app, "Sedlo");
    const before = await coreLines(id);
    const right = Math.max(...(await loadProject(id)).parts.flatMap((pt: any) => pt.edges.map((e: any) => e.start.x)));
    // Right click inside the main building, then Duplicate.
    const box = (await plan(page).boundingBox())!;
    const at = await planXY(page, 3, 3);
    expect(at.x).toBeGreaterThan(box.x);
    await page.mouse.click(at.x, at.y, { button: "right" });
    await page.getByRole("menuitem", { name: cs("menu.duplicatePart") }).click();
    const p = await waitForProject(id, (p) => p.parts.length === 3);
    const copyLeft = Math.min(...p.parts[2].edges.map((e: any) => e.start.x));
    expect(copyLeft).toBeGreaterThan(right);
    // The other buildings' roofs are as they were.
    const after = await coreLines(id);
    expect(after.slice(0, 2)).toEqual(before);
    await areaMatchesCore(page, id);
  });

  test("labels of buildings drawn into each other do not overlap (UX-012)", async ({ app, page }) => {
    const id = await createExample("gable", "Sedlo");
    // A lower hip roof overlapping the gable's end, and one inside it.
    await withSecondPart(id, rect(9, 2, 15, 6, 2.5));
    await openEditor(app, "Sedlo");
    await expect.poll(async () => (await labelBoxes(page)).length).toBeGreaterThan(4);
    const boxes = await labelBoxes(page);
    const bad: string[] = [];
    for (let i = 0; i < boxes.length; i++) {
      for (let j = i + 1; j < boxes.length; j++) {
        const [a, b] = [boxes[i], boxes[j]];
        if (Math.min(a.x1, b.x1) - Math.max(a.x0, b.x0) > 2 && Math.min(a.y1, b.y1) - Math.max(a.y0, b.y0) > 2) bad.push(`${a.t} / ${b.t}`);
      }
    }
    expect(bad).toEqual([]);
    // Wall lengths use the UI language's decimal comma (UX-008).
    const dims = await page.locator(".edge-dim").allTextContents();
    expect(dims.length).toBeGreaterThan(0);
    for (const d of dims) expect(d, d).toMatch(/^\d+\s+\d+,\d\d$/);
    await areaMatchesCore(page, id);
  });
});
