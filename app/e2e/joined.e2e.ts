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
