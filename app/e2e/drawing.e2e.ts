// Drawing footprints: clicks in the plan, typed lengths with the arrow
// buttons and keys, undo point, cancel, close, presets and the Insert key.
import { clickPlan, openEditor, setField, shownArea, tool } from "./editor";
import { createExample, expect, listProjects, loadProject, test, waitForProject } from "./fixtures";
import { cs } from "./i18n";

async function newDrawnJob(app: import("./fixtures").App) {
  const page = app.page;
  await app.open();
  await page.locator("header").getByRole("button", { name: cs("lib.new") }).click();
  const dlg = page.getByRole("dialog", { name: cs("new.title") });
  await dlg.getByLabel(cs("job.name")).fill("Kreslená");
  await dlg.getByRole("tab", { name: cs("new.draw") }).click();
  await dlg.getByRole("button", { name: cs("new.create") }).click();
  const draw = page.getByRole("toolbar", { name: cs("draw.title") });
  await expect(draw).toBeVisible();
  return { draw, id: (await listProjects())[0].id as string };
}

test.describe("drawing a footprint", () => {
  test("first corner by click, walls by length + arrow keys, Enter closes", async ({ app, page }) => {
    const { draw, id } = await newDrawnJob(app);
    await expect(draw).toContainText(cs("draw.start"));
    await expect(draw.getByRole("button", { name: cs("draw.close") })).toBeDisabled();
    await clickPlan(page, 2, 2);
    await expect(draw).toContainText(cs("draw.points", { n: 1 }));
    const len = draw.getByLabel(cs("draw.length"));
    await len.fill("10");
    await len.press("ArrowRight");
    await len.fill("6,5");
    await len.press("ArrowUp");
    await len.fill("10");
    await len.press("ArrowLeft");
    await expect(draw).toContainText(cs("draw.points", { n: 4 }));
    await len.press("Enter");
    await expect(draw).toHaveCount(0);
    const p = await waitForProject(id, (p) => p.parts.length === 1);
    expect(p.parts[0].edges.map((e: any) => e.start)).toEqual([
      { x: 2, y: 2 },
      { x: 12, y: 2 },
      { x: 12, y: 8.5 },
      { x: 2, y: 8.5 },
    ]);
    expect(p.parts[0].name).toBe(cs("editor.mainPart"));
    await expect.poll(() => shownArea(page)).toBeGreaterThan(65);
  });

  test("arrow buttons in all eight directions, undo point, close with the button", async ({ app, page }) => {
    const { draw, id } = await newDrawnJob(app);
    await clickPlan(page, 0, 0);
    const len = draw.getByLabel(cs("draw.length"));
    const arrows = draw.locator(".arrows button");
    await expect(arrows).toHaveCount(8);
    await len.fill("4");
    await arrows.nth(2).click(); // →
    await arrows.nth(5).click(); // ↗
    await arrows.nth(1).click(); // ↑
    await arrows.nth(4).click(); // ↖
    await arrows.nth(0).click(); // ←
    await arrows.nth(6).click(); // ↙
    await arrows.nth(3).click(); // ↓
    await expect(draw).toContainText(cs("draw.points", { n: 8 }));
    await draw.getByRole("button", { name: cs("draw.undo") }).click();
    await expect(draw).toContainText(cs("draw.points", { n: 7 }));
    await draw.getByRole("button", { name: cs("draw.close") }).click();
    const p = await waitForProject(id, (p) => p.parts.length === 1);
    expect(p.parts[0].edges).toHaveLength(7);
    const s = Math.SQRT1_2 * 4;
    expect(p.parts[0].edges[2].start.x).toBeCloseTo(4 + s, 3);
    expect(p.parts[0].edges[2].start.y).toBeCloseTo(s, 3);
  });

  test("clicking corners and the first corner again closes; Backspace removes a point", async ({ app, page }) => {
    const { draw, id } = await newDrawnJob(app);
    await clickPlan(page, 0, 0);
    await clickPlan(page, 8, 0);
    await clickPlan(page, 9, 9);
    await page.keyboard.press("Backspace");
    await expect(draw).toContainText(cs("draw.points", { n: 2 }));
    await clickPlan(page, 8, 6);
    await clickPlan(page, 0, 6);
    await clickPlan(page, 0, 0);
    await expect(draw).toHaveCount(0);
    const p = await waitForProject(id, (p) => p.parts.length === 1);
    expect(p.parts[0].edges.map((e: any) => [e.start.x, e.start.y])).toEqual([
      [0, 0],
      [8, 0],
      [8, 6],
      [0, 6],
    ]);
  });

  test("a footprint drawn clockwise is stored counter-clockwise", async ({ app, page }) => {
    const { draw, id } = await newDrawnJob(app);
    await clickPlan(page, 0, 0);
    const len = draw.getByLabel(cs("draw.length"));
    await len.fill("5");
    await len.press("ArrowUp");
    await len.press("ArrowRight");
    await len.press("ArrowDown");
    await len.press("Enter");
    const p = await waitForProject(id, (p) => p.parts.length === 1);
    const pts = p.parts[0].edges.map((e: any) => e.start);
    const area = pts.reduce((s: number, a: any, i: number) => {
      const b = pts[(i + 1) % pts.length];
      return s + a.x * b.y - b.x * a.y;
    }, 0);
    expect(area).toBeGreaterThan(0);
  });

  test("presets set pitch, eave height and overhang of the new walls", async ({ app, page }) => {
    const { draw, id } = await newDrawnJob(app);
    await setField(draw.getByLabel(cs("edge.pitch")), 22);
    await setField(draw.getByLabel(cs("edge.eaveHeight")), 2.75);
    await setField(draw.getByLabel(cs("edge.overhang")), 0.35);
    await clickPlan(page, 0, 0);
    const len = draw.getByLabel(cs("draw.length"));
    await len.fill("6");
    await len.press("ArrowRight");
    await len.press("ArrowUp");
    await len.press("ArrowLeft");
    await len.press("Enter");
    const p = await waitForProject(id, (p) => p.parts.length === 1);
    for (const e of p.parts[0].edges) expect(e).toMatchObject({ pitch_deg: 22, eave_height: 2.75, overhang: 0.35 });
  });

  test("Escape and Cancel stop drawing without adding a part", async ({ app, page }) => {
    const { draw, id } = await newDrawnJob(app);
    await clickPlan(page, 0, 0);
    await clickPlan(page, 5, 0);
    await page.keyboard.press("Escape");
    await expect(draw).toHaveCount(0);
    await tool(page, "tool.draw").click();
    await clickPlan(page, 0, 0);
    await draw.getByRole("button", { name: cs("draw.cancel") }).click();
    await expect(draw).toHaveCount(0);
    await page.waitForTimeout(700);
    expect((await loadProject(id)).parts).toEqual([]);
  });

  test("Walls tool and the Insert key add a second part next to the first", async ({ app, page }) => {
    const id = await createExample("gable", "Sedlo");
    await openEditor(app, "Sedlo");
    await page.locator("body").press("Insert");
    const draw = page.getByRole("toolbar", { name: cs("draw.title") });
    await expect(draw).toBeVisible();
    await clickPlan(page, 13, 1);
    const len = draw.getByLabel(cs("draw.length"));
    await len.fill("4");
    await len.press("ArrowRight");
    await len.press("ArrowUp");
    await len.press("ArrowLeft");
    await len.press("Enter");
    const p = await waitForProject(id, (p) => p.parts.length === 2);
    expect(p.parts[1].name).toBe(cs("editor.partN", { n: 2 }));
    await expect(page.locator(".statusbar")).toContainText(cs("status.walls", { n: 8, p: 2 }));
    await expect(page.locator(".results")).toContainText(cs("res.areas"));
    // Undo removes the whole new part in one step.
    await tool(page, "editor.undo").click();
    await waitForProject(id, (p) => p.parts.length === 1);
  });
});

test.describe("standard roof tool", () => {
  test("replace the selected part, or add a new part beside it", async ({ app, page }) => {
    const id = await createExample("gable", "Sedlo");
    await openEditor(app, "Sedlo");
    await tool(page, "tool.standard").click();
    const dlg = page.getByRole("dialog", { name: cs("std.title") });
    await dlg.getByRole("radio", { name: cs("std.hip"), exact: true }).click();
    await setField(dlg.getByLabel(cs("std.length")), 10);
    await setField(dlg.getByLabel(cs("std.width")), 7);
    await expect(dlg.getByLabel(cs("std.replace"))).toBeChecked();
    await dlg.getByRole("button", { name: cs("common.ok") }).click();
    let p = await waitForProject(id, (p) => p.parts[0].edges.every((e: any) => e.pitch_deg !== 90));
    expect(p.parts).toHaveLength(1);
    expect(p.parts[0].edges[1].start).toEqual({ x: 10, y: 0 });

    await tool(page, "tool.standard").click();
    await dlg.getByRole("radio", { name: cs("std.shed"), exact: true }).click();
    await dlg.getByLabel(cs("std.add")).check();
    await dlg.getByRole("button", { name: cs("common.ok") }).click();
    p = await waitForProject(id, (p) => p.parts.length === 2);
    expect(Math.min(...p.parts[1].edges.map((e: any) => e.start.x))).toBe(12);

    await tool(page, "tool.standard").click();
    await dlg.getByRole("button", { name: cs("common.cancel") }).click();
    await expect(dlg).toHaveCount(0);
  });
});
