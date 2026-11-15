// The plan and the bars under it, from the 2026-11-14 review: eaves drawn at
// the overhang, walls stepped by keyboard, footprint walls by length and
// angle with their lengths shown, the placing tools' own bar, two buildings'
// wall labels kept apart and numbered per building, radio groups moved with
// the arrow keys, and the transform dialog's copy limit said.
import type { Page } from "@playwright/test";
import { bar, clickPlan, openEditor, plan, selectWall, tool } from "./editor";
import { bridge, createExample, expect, listProjects, loadProject, test, waitForProject } from "./fixtures";
import { cs } from "./i18n";

/** Screen extent of the plan elements matching `sel` (lines): [x0, y0, x1, y1]. */
const extent = (page: Page, sel: string) =>
  plan(page)
    .locator(sel)
    .evaluateAll((ls) => {
      const xs: number[] = [];
      const ys: number[] = [];
      for (const l of ls) {
        xs.push(+l.getAttribute("x1")!, +l.getAttribute("x2")!);
        ys.push(+l.getAttribute("y1")!, +l.getAttribute("y2")!);
      }
      return [Math.min(...xs), Math.min(...ys), Math.max(...xs), Math.max(...ys)];
    });

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

test.describe("plan", () => {
  test("the eave is drawn at the overhang and walls are not in the eave colour (UX-004)", async ({ app, page }) => {
    await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    const eaves = plan(page).locator(".roofline.eave");
    await expect.poll(() => eaves.count()).toBeGreaterThanOrEqual(4);
    // 0.5 m overhang all round: the eave lines lie outside the walls on every side.
    const [e, w] = [await extent(page, ".roofline.eave"), await extent(page, "g.edge line.wall")];
    expect(e[0]).toBeLessThan(w[0] - 3);
    expect(e[1]).toBeLessThan(w[1] - 3);
    expect(e[2]).toBeGreaterThan(w[2] + 3);
    expect(e[3]).toBeGreaterThan(w[3] + 3);
    const eaveStroke = await eaves.first().evaluate((l) => getComputedStyle(l).stroke);
    const wallStroke = await plan(page).locator("g.edge line.wall").nth(1).evaluate((l) => getComputedStyle(l).stroke);
    expect(wallStroke).not.toBe(eaveStroke);
  });

  test("walls drawn so far show their lengths while drawing (UX-014)", async ({ app, page }) => {
    const { draw } = await newDrawnJob(app);
    await clickPlan(page, 2, 2);
    const len = draw.getByLabel(cs("draw.length"));
    await len.fill("10");
    await len.press("ArrowRight");
    await len.fill("7,5");
    await len.press("ArrowUp");
    await expect(plan(page).getByTestId("draft-wall-dim")).toHaveText(["10,00", "7,50"]);
  });

  test("two buildings side by side: wall labels apart and numbered per building (UX-018)", async ({ app, page }) => {
    const id = await createExample("gable", "Sedlo");
    const p = await loadProject(id);
    p.parts.push({
      name: "P2",
      edges: [
        [12.3, 1],
        [17.3, 1],
        [17.3, 7],
        [12.3, 7],
      ].map(([x, y]) => ({ start: { x, y }, pitch_deg: 40, eave_height: 3, overhang: 0.5 })),
    });
    await bridge("save_project", { id, project: p });
    await openEditor(app, "Sedlo");
    const dims = plan(page).locator(".edge-dim");
    await expect.poll(() => dims.count()).toBeGreaterThan(6);
    const texts = await dims.allTextContents();
    for (const d of texts) expect(d, d).toMatch(/^\d+\.\d+\s+\d+,\d\d$/);
    // The two walls facing each other across the 0.3 m gap both keep their length.
    expect(texts.some((d) => d.startsWith("1.2 "))).toBe(true);
    expect(texts.some((d) => d.startsWith("2.4 "))).toBe(true);
    const boxes = await plan(page)
      .locator(".edge-dim, .face-tag")
      .evaluateAll((els) => els.map((e) => ({ t: e.textContent, r: e.getBoundingClientRect() })).map(({ t, r }) => ({ t, x0: r.left, y0: r.top, x1: r.right, y1: r.bottom })));
    const bad: string[] = [];
    for (let i = 0; i < boxes.length; i++)
      for (let j = i + 1; j < boxes.length; j++) {
        const [a, b] = [boxes[i], boxes[j]];
        if (Math.min(a.x1, b.x1) - Math.max(a.x0, b.x0) > 2 && Math.min(a.y1, b.y1) - Math.max(a.y0, b.y0) > 2) bad.push(`${a.t} / ${b.t}`);
      }
    expect(bad).toEqual([]);
  });
});

test.describe("keyboard", () => {
  test("PageDown / PageUp and Alt+arrows step walls; in a field they store first and keep the field (UX-006)", async ({ app, page }) => {
    const id = await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    const def = bar(page, "def.title");
    const which = def.locator(".which strong");
    await selectWall(page, 0);
    await page.keyboard.press("PageDown");
    await expect(which).toHaveText(cs("def.wall", { n: 2 }));
    await page.keyboard.press("Alt+ArrowRight");
    await expect(which).toHaveText(cs("def.wall", { n: 3 }));
    await page.keyboard.press("PageUp");
    await expect(which).toHaveText(cs("def.wall", { n: 2 }));
    await page.keyboard.press("Alt+ArrowLeft");
    await expect(which).toHaveText(cs("def.wall", { n: 1 }));

    const pitch = def.getByLabel(cs("edge.pitch")).first();
    await pitch.fill("35");
    await pitch.press("PageDown");
    await expect(which).toHaveText(cs("def.wall", { n: 2 }));
    await expect(pitch).toBeFocused();
    await pitch.fill("36");
    await pitch.press("Shift+Enter");
    await expect(which).toHaveText(cs("def.wall", { n: 3 }));
    await expect(pitch).toBeFocused();
    const p = await waitForProject(id, (p) => p.parts[0].edges[0].pitch_deg === 35 && p.parts[0].edges[1].pitch_deg === 36);
    expect(p.parts[0].edges[2].pitch_deg).not.toBe(36);
    // The keys are named in the buttons' tooltips.
    await expect(def.getByRole("button", { name: cs("def.next") })).toHaveAttribute("title", /PageDown/);
  });

  test("radio groups: one Tab stop, the arrow keys move and select (UI-007)", async ({ app, page }) => {
    const id = await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    const views = page.getByRole("radiogroup", { name: cs("view.title") });
    const radios = views.getByRole("radio");
    await radios.first().click();
    await expect(views.locator('[tabindex="0"]')).toHaveCount(1);
    await page.keyboard.press("ArrowRight");
    await expect(radios.nth(1)).toHaveAttribute("aria-checked", "true");
    await expect(radios.nth(1)).toBeFocused();
    await expect(views.locator('[tabindex="0"]')).toHaveCount(1);

    await selectWall(page, 0);
    const type = bar(page, "def.title").getByRole("radiogroup", { name: cs("edge.type") });
    await type.getByRole("radio", { name: cs("edge.eave") }).focus();
    await page.keyboard.press("ArrowRight");
    await expect(type.getByRole("radio", { name: cs("edge.gable") })).toBeFocused();
    await waitForProject(id, (p) => p.parts[0].edges[0].pitch_deg === 90);
  });
});

test.describe("drawing and placing", () => {
  test("a wall by length and typed angle, absolute or from the previous wall (UX-007)", async ({ app, page }) => {
    const { draw, id } = await newDrawnJob(app);
    await clickPlan(page, 0, 0);
    const len = draw.getByLabel(cs("draw.length"));
    const angle = draw.getByTestId("draw-angle");
    await len.fill("10");
    await angle.fill("0");
    await angle.press("Enter");
    await len.fill("5");
    await angle.fill("90");
    await angle.press("Enter");
    await expect(draw).toContainText(cs("draw.points", { n: 3 }));
    // From the previous wall (up): +90° turns left. Enter in the angle field
    // adds the wall, it does not close the footprint.
    await draw.getByTestId("draw-relative").check();
    await len.fill("4");
    await angle.fill("90");
    await angle.press("Enter");
    await expect(draw).toContainText(cs("draw.points", { n: 4 }));
    // 30° turn to the right from "left": up and to the left.
    await len.fill("2");
    await angle.fill("-30");
    await draw.getByTestId("draw-angle-go").click();
    await expect(draw).toContainText(cs("draw.points", { n: 5 }));
    await len.press("Enter");
    const p = await waitForProject(id, (p) => p.parts.length === 1);
    const pts = p.parts[0].edges.map((e: any) => [e.start.x, e.start.y]);
    expect(pts).toEqual([
      [0, 0],
      [10, 0],
      [10, 5],
      [6, 5],
      [4.268, 6],
    ]);
  });

  test("the manual face tool shows its own bar with steps, points, undo and Cancel (UX-012)", async ({ app, page }) => {
    await createExample("gable", "Sedlo");
    await openEditor(app, "Sedlo");
    await tool(page, "tool.face").click();
    const pb = page.getByTestId("place-bar");
    await expect(pb).toBeVisible();
    await expect(pb).toContainText(cs("tool.face"));
    await expect(pb.getByTestId("place-steps")).toHaveText(cs("plan.placeFace"));
    await expect(bar(page, "def.title")).toHaveCount(0);
    await clickPlan(page, 2, 2);
    await clickPlan(page, 5, 2);
    await expect(pb.getByTestId("place-points")).toHaveText(cs("place.points", { n: 2 }));
    await pb.getByRole("button", { name: cs("draw.undo") }).click();
    await expect(pb.getByTestId("place-points")).toHaveText(cs("place.points", { n: 1 }));
    await pb.getByRole("button", { name: cs("common.cancel") }).click();
    await expect(pb).toHaveCount(0);
    await expect(bar(page, "def.title")).toBeVisible();

    await tool(page, "tool.line").click();
    await expect(pb.getByTestId("place-steps")).toHaveText(cs("plan.placeLine"));
    await page.keyboard.press("Escape");
    await expect(pb).toHaveCount(0);
  });

  test("Transform refuses more copies than it makes and says the limit (UI-009)", async ({ app, page }) => {
    await createExample("gable", "Sedlo");
    await openEditor(app, "Sedlo");
    await selectWall(page, 0);
    await tool(page, "tool.transform").click();
    const dlg = page.getByRole("dialog", { name: cs("xf.title") });
    const copies = dlg.getByTestId("xf-copies");
    await expect(dlg).toContainText(cs("xf.copiesMax", { n: 50 }));
    await copies.fill("120");
    await expect(copies).toHaveAttribute("aria-invalid", "true");
    await expect(copies).toHaveAttribute("title", cs("num.range", { min: "0", max: "50" }));
    await copies.fill("3");
    await expect(copies).not.toHaveAttribute("aria-invalid", "true");
    await dlg.getByRole("radio", { name: cs("xf.mirror") }).click();
    await expect(copies).toHaveValue("1");
    await expect(dlg).toContainText(cs("xf.copiesMax", { n: 1 }));
  });
});
