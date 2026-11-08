// Several buildings in one job: a second building gets its roof at once, the
// first building's gable stays intact, joined roofs say what they are joined
// to, lines under a higher roof are dashed, and a building hidden inside
// another is reported instead of silently showing no roof.
import { bar, clickPlan, openEditor, selectWall, setField, tool } from "./editor";
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
    const main = (await loadProject(id)).parts[0].name;
    await expect(results).toContainText(cs("res.joinedTo", { names: main }));
    // The second roof's stretches under the main roof's overhang are dashed.
    await expect(page.locator(".roofline.covered").first()).toBeAttached();
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
    await setField(def.getByLabel(cs("edge.eaveHeight")), "4,5");
    await waitForProject(id, (p) => p.parts[1].edges[0].eave_height === 4.5);
    await expect.poll(lines).not.toBe(shape);
    await expect(area).not.toHaveText("");
  });
});
