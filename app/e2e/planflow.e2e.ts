// The plan and the bars under it, from the 2026-11-14 review: eaves drawn at
// the overhang, walls stepped by keyboard, footprint walls by length and
// angle with their lengths shown, the placing tools' own bar, two buildings'
// wall labels kept apart and numbered per building, radio groups moved with
// the arrow keys, and the transform dialog's copy limit said; from the
// 2026-11-15 sweep: labels of several buildings off roof lines and walls,
// and PageDown / PageUp for the object bars.
import type { Page } from "@playwright/test";
import { bar, clickPlan, openEditor, plan, selectWall, tool } from "./editor";
import { bridge, createExample, expect, listProjects, loadProject, test, waitForProject } from "./fixtures";
import { cs, tr } from "./i18n";

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

test.describe("labels of several buildings", () => {
  test("wall lengths keep off the ridge and face labels off the walls where there is room (sweep 2026-11-15)", async ({ app, page }) => {
    const id = await createExample("gable", "Dvě budovy");
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
    await openEditor(app, "Dvě budovy");
    await expect.poll(() => plan(page).locator(".edge-dim").count()).toBeGreaterThan(6);
    // Label boxes (shrunk by 3 px) against the plan's lines, in plan pixels.
    const crossings = (labels: string, lines: string) =>
      plan(page).evaluate(
        (svg, [labels, lines]) => {
          const o = svg.getBoundingClientRect();
          const segs = [...svg.querySelectorAll(lines)].map((l) => ["x1", "y1", "x2", "y2"].map((a) => +l.getAttribute(a)!));
          const hit = (s: number[], b: number[]) => {
            let [t0, t1] = [0, 1];
            const [dx, dy] = [s[2] - s[0], s[3] - s[1]];
            for (const [pp, q] of [[-dx, s[0] - b[0]], [dx, b[2] - s[0]], [-dy, s[1] - b[1]], [dy, b[3] - s[1]]]) {
              if (pp === 0) {
                if (q < 0) return false;
              } else if (pp < 0) t0 = Math.max(t0, q / pp);
              else t1 = Math.min(t1, q / pp);
              if (t0 > t1) return false;
            }
            return true;
          };
          const bad: string[] = [];
          for (const el of svg.querySelectorAll(labels)) {
            const r = el.getBoundingClientRect();
            const b = [r.left - o.left + 3, r.top - o.top + 3, r.right - o.left - 3, r.bottom - o.top - 3];
            if (segs.some((sg) => hit(sg, b))) bad.push(el.textContent ?? "");
          }
          return bad;
        },
        [labels, lines] as const,
      );
    // The shared wall "1.2" had its length on the ridge, the small
    // building's rotated "40°" on its own wall beside "2.2". (Inside the
    // small hip roof "2.4" has no spot clear of every line: it takes the
    // one on the fewest, its short ridge.)
    const onRidge = await crossings(".edge-dim", 'line.roofline[stroke="var(--line-ridge)"]');
    expect(onRidge.filter((d) => d.startsWith("1."))).toEqual([]);
    expect(await crossings(".face-tag", "g.edge line.wall")).toEqual([]);
    const texts = await plan(page).locator(".edge-dim").allTextContents();
    expect(texts.some((d) => d.startsWith("1.2 "))).toBe(true);
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

  test("PageDown / PageUp step dormers, windows and hand-drawn lines; in a field they store first and keep the field", async ({ app, page }) => {
    const id = await createExample("gable", "Objekty");
    const p = await loadProject(id);
    const dormer = (x: number) => ({
      edges: [
        { start: { x, y: 1.5 }, pitch_deg: 45, eave_height: 4.5, overhang: 0, direction_rad: 4.71238898038469 },
        { start: { x, y: 1.5 }, pitch_deg: 90, eave_height: 4.5, overhang: 0 },
        { start: { x: x + 1.5, y: 1.5 }, pitch_deg: 45, eave_height: 4.5, overhang: 0, direction_rad: 1.5707963267948966 },
      ],
      end: { x: x + 1.5, y: 1.5 },
    });
    p.dormers = [dormer(2), dormer(7)];
    p.windows = [
      { x: 3, y: 6, width: 0.78, height: 1.18 },
      { x: 8, y: 6, width: 0.78, height: 1.18 },
    ];
    p.manual_lines = [
      { a: [1, 3, 0], b: [4, 3, 0], code: "M" },
      { a: [6, 3, 0], b: [9, 3, 0], code: "M" },
    ];
    await bridge("save_project", { id, project: p });
    await openEditor(app, "Objekty");
    const pick = (key: string, n: number) => plan(page).getByRole("button", { name: cs(key, { n }) }).first().dispatchEvent("pointerdown", { button: 0 });

    // Dormers, from the plan: the keys step and wrap; the tooltips name them.
    await pick("plan.dormer", 1);
    const db = bar(page, "dormer.title");
    await expect(db.locator(".which strong")).toHaveText(cs("dormer.n", { n: 1 }));
    await expect(db.getByRole("button", { name: cs("dormer.next") })).toHaveAttribute("title", /PageDown/);
    await expect(db.getByRole("button", { name: cs("dormer.prev") })).toHaveAttribute("title", /PageUp/);
    await page.keyboard.press("PageDown");
    await expect(db.locator(".which strong")).toHaveText(cs("dormer.n", { n: 2 }));
    await page.keyboard.press("PageDown");
    await expect(db.locator(".which strong")).toHaveText(cs("dormer.n", { n: 1 }));
    await page.keyboard.press("PageUp");
    await expect(db.locator(".which strong")).toHaveText(cs("dormer.n", { n: 2 }));

    // Windows: a width typed in the field is stored on window 1, then the
    // same field of window 2 has the focus.
    await pick("opening.windowN", 1);
    const ob = bar(page, "opening.window");
    await expect(ob.locator(".which strong")).toHaveText(cs("opening.windowN", { n: 1 }));
    await expect(ob.getByRole("button", { name: cs("opening.next") })).toHaveAttribute("title", /PageDown/);
    const width = ob.getByLabel(cs("opening.width"));
    await width.fill("1,14");
    await width.press("PageDown");
    await expect(ob.locator(".which strong")).toHaveText(cs("opening.windowN", { n: 2 }));
    await expect(ob.getByLabel(cs("opening.width"))).toBeFocused();
    await waitForProject(id, (q) => q.windows[0].width === 1.14 && q.windows[1].width === 0.78);
    await page.keyboard.press("PageUp");
    await expect(ob.locator(".which strong")).toHaveText(cs("opening.windowN", { n: 1 }));

    // Hand-drawn lines.
    await pick("manual.lineN", 2);
    const mb = bar(page, "manual.line");
    await expect(mb.locator(".which strong")).toHaveText(cs("manual.lineN", { n: 2 }));
    await expect(mb.getByRole("button", { name: cs("opening.prev") })).toHaveAttribute("title", /PageUp/);
    await page.keyboard.press("PageDown");
    await expect(mb.locator(".which strong")).toHaveText(cs("manual.lineN", { n: 1 }));
    // A modified key is not a step.
    await page.keyboard.press("Shift+PageDown");
    await expect(mb.locator(".which strong")).toHaveText(cs("manual.lineN", { n: 1 }));
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

  for (const loc of ["de", "sk"] as const) {
    test(`the draw bar's buttons fit a 600 × 420 window in ${loc} (sweep 2026-11-15)`, async ({ app, page }) => {
      await createExample("gable", "Úzké okno");
      await app.open({ locale: loc });
      await app.openJob("Úzké okno");
      await page.setViewportSize({ width: 600, height: 420 });
      await page.getByRole("toolbar").first().getByRole("button", { name: tr(loc, "tool.draw"), exact: true }).click();
      const draw = page.getByRole("toolbar", { name: tr(loc, "draw.title") });
      await clickPlan(page, 2, 2);
      for (const key of ["draw.undo", "draw.cancel", "draw.close"]) {
        const b = draw.getByRole("button", { name: tr(loc, key), exact: true });
        await expect(b).toBeVisible();
        const r = (await b.boundingBox())!;
        expect(r.x + r.width, `${loc} ${key}`).toBeLessThanOrEqual(600);
        // The label is not cut (an ellipsis is the last resort, not the rule).
        const cut = await b.evaluate((el) => [...el.querySelectorAll<HTMLElement>(".txt")].some((s) => s.offsetWidth > 1 && s.scrollWidth > s.clientWidth + 1));
        expect(cut, `${loc} ${key}`).toBe(false);
      }
      await page.keyboard.press("Escape");
    });
  }

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
