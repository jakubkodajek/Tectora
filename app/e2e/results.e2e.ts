// The Results tab: the area list names everything the total counts (parts
// and hand-drawn faces), a part's name stands apart from its warning, and
// objects the calculation leaves out are reported.
import { openEditor, shownArea } from "./editor";
import { bridge, createExample, expect, loadProject, test } from "./fixtures";
import { cs } from "./i18n";

/** A figure as the Czech UI shows it ("1 234,56 m²") as a number. */
function figure(text: string): number {
  return Number(text.replace(/m²/g, "").replace(/\s/g, "").replace(",", "."));
}

function rect(x0: number, y0: number, x1: number, y1: number, name: string) {
  return {
    name,
    edges: [
      [x0, y0],
      [x1, y0],
      [x1, y1],
      [x0, y1],
    ].map(([x, y]) => ({ start: { x, y }, pitch_deg: 40, eave_height: 3, overhang: 0.5 })),
  };
}

test.describe("results", () => {
  test("the area list has a row for each hand-drawn face and the rows add up to the total (UX-009)", async ({ app, page }) => {
    const id = await createExample("gable", "Sedlo");
    const p = await loadProject(id);
    p.manual_faces = [
      { points: [{ x: 16, y: 0 }, { x: 20, y: 0 }, { x: 20, y: 4 }, { x: 16, y: 4 }], pitch_deg: 30 },
      { points: [{ x: 22, y: 0 }, { x: 24, y: 0 }, { x: 24, y: 3 }], pitch_deg: 0, label: "Stříška" },
    ];
    await bridge("save_project", { id, project: p });
    const core = await bridge<any>("calc", { project: await loadProject(id) });
    await openEditor(app, "Sedlo");
    const rows = page.locator(".results table.areas tr");
    await expect(rows).toHaveCount(3);
    await expect(rows.nth(1)).toContainText(cs("manual.faceN", { n: 1 }));
    await expect(rows.nth(2)).toContainText("Stříška");
    const values = await page.locator(".results table.areas td.v").allTextContents();
    const listed = values.map(figure);
    expect(listed[1]).toBeCloseTo(core.manual_faces[0].area, 2);
    expect(listed[2]).toBeCloseTo(3, 2);
    // The rows add up to the total the hero shows (to the rounding of the shown figures).
    const total = await shownArea(page);
    expect(total).toBeCloseTo(core.totals.roof_area, 2);
    expect(Math.abs(listed.reduce((s, v) => s + v, 0) - total)).toBeLessThanOrEqual(0.005 * listed.length + 1e-9);
  });

  test("a part's name stands apart from its warning", async ({ app, page }) => {
    // "Část 2Tato část…": the name ran into the sentence.
    const id = await createExample("gable", "Sedlo");
    const p = await loadProject(id);
    p.parts.push(rect(4, 2, 7, 5, "Vnitřní"));
    await bridge("save_project", { id, project: p });
    await openEditor(app, "Sedlo");
    const warning = page.locator(".results .warn p").filter({ hasText: cs("res.warn.covered") });
    await expect(warning).toHaveCount(1);
    const text = ((await warning.textContent()) ?? "").replace(/\s+/g, " ").trim();
    expect(text).toBe(`Vnitřní ${cs("res.warn.covered")}`);
  });

  test("a roof window on no roof face is reported and the area stays (re-check of ERR-011)", async ({ app, page }) => {
    const id = await createExample("gable", "Sedlo");
    const before = await bridge<any>("calc", { project: await loadProject(id) });
    const p = await loadProject(id);
    p.windows = [{ x: -50, y: -50, width: 0.78, height: 1.18 }];
    await bridge("save_project", { id, project: p });
    await openEditor(app, "Sedlo");
    await expect(page.locator(".results")).toContainText(cs("res.warn.window_off_roof", { n: 1 }));
    expect(await shownArea(page)).toBeCloseTo(before.totals.roof_area, 2);
  });
});
