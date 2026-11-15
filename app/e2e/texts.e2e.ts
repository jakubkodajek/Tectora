// Results, export and AI review texts (review sweep 2026-11-14): the Export
// dialog shows the price-sheet list translated and copies it unchanged
// (UX-003), wall abutments are named apart and every code on screen is in
// the legend (UX-013), the deductions read as deductions (UX-015), the AI's
// previous roof is drawn for a pitch change (UX-017), German tells Undo
// and the AI's Revert apart (UX-019) and the More row says what the eave
// height is (UX-008). Set TECTORA_SHOTS_DIR to keep a screenshot of each.
import fs from "node:fs";
import path from "node:path";
import type { Page } from "@playwright/test";
import { bar, openEditor, selectWall } from "./editor";
import { bridge, createExample, expect, loadProject, test } from "./fixtures";
import { cs, tr } from "./i18n";

async function shot(page: Page, name: string) {
  const dir = process.env.TECTORA_SHOTS_DIR;
  if (!dir) return;
  fs.mkdirSync(dir, { recursive: true });
  await page.screenshot({ path: path.join(dir, name) });
}

const edge = (x: number, y: number, pitch: number) => ({ start: { x, y }, pitch_deg: pitch, eave_height: 3, overhang: 0.5 });

/** The job of review UX-011 / UX-013: a gable roof with a dormer, a chimney and a roof window. */
async function crowdedJob(name: string): Promise<string> {
  const id = await createExample("gable", name);
  const p = await loadProject(id);
  p.parts = [{ name: p.parts[0].name, edges: [edge(0, 0, 40), edge(12, 0, 90), edge(12, 8, 40), edge(0, 8, 90)] }];
  p.dormers = [
    {
      edges: [
        { start: { x: 6, y: 2 }, pitch_deg: 45, eave_height: 6.11, overhang: 0, direction_rad: 4.71238898038469 },
        { start: { x: 6, y: 2 }, pitch_deg: 90, eave_height: 6.11, overhang: 0 },
        { start: { x: 8, y: 2 }, pitch_deg: 45, eave_height: 6.11, overhang: 0, direction_rad: 1.5707963267948966 },
      ],
      end: { x: 8, y: 2 },
    },
  ];
  p.chimneys = [{ corners: [{ x: 3, y: 6 }, { x: 2.4, y: 6 }, { x: 2.4, y: 5.4 }, { x: 3, y: 5.4 }], height: 5.85 }];
  p.windows = [{ x: 9, y: 6, width: 0.78, height: 1.18 }];
  await bridge("save_project", { id, project: p });
  return id;
}

test.describe("results and export texts", () => {
  test("the Export dialog shows the list translated and copies the price sheet's text unchanged (UX-003)", async ({ app, page }) => {
    await crowdedJob("Vikýř export");
    await page.setViewportSize({ width: 1360, height: 860 });
    await openEditor(app, "Vikýř export");
    await page.getByRole("button", { name: cs("tool.export") }).click();
    const dlg = page.getByRole("dialog", { name: cs("export.title") });
    const sum = dlg.getByTestId("export-summary");
    await expect(sum).toBeVisible();
    await expect(sum).toContainText(cs("export.sumFaces"));
    await expect(sum).toContainText(cs("line.WV"));
    await expect(sum).toContainText("m²");
    // Nothing of the price sheet's own words is on screen in Czech ...
    const raw = dlg.getByTestId("export-list");
    await expect(raw).toBeHidden();
    for (const word of ["Fenster", "Breite", "STREŠNÉ", "Dĺžka", "m˛"]) await expect(sum).not.toContainText(word);
    // ... but the copied text is the core's list, byte for byte.
    await page.context().grantPermissions(["clipboard-read", "clipboard-write"]);
    await dlg.getByRole("button", { name: cs("export.copyList") }).click();
    const copied = await page.evaluate(() => navigator.clipboard.readText());
    expect(copied).toBe(await raw.textContent());
    expect(copied).toContain(" Fenster1.");
    expect(copied).toContain("STREŠNÉ PLOCHY");
    await shot(page, "ux003-export-summary_cs_1360.png");
    // The disclosure shows the text as copied, in monospace.
    await dlg.getByText(cs("export.rawText")).click();
    await expect(raw).toBeVisible();
    await expect(dlg).toContainText(cs("export.rawNote"));
    await shot(page, "ux003-export-raw_cs_1360.png");
  });

  test("the Export dialog fits a 600 × 420 window in German (UX-003)", async ({ app, page }) => {
    await crowdedJob("Gaube");
    await page.setViewportSize({ width: 600, height: 420 });
    await app.open({ locale: "de" });
    await app.openJob("Gaube");
    await page.getByRole("button", { name: tr("de", "tool.export") }).click();
    const dlg = page.getByRole("dialog", { name: tr("de", "export.title") });
    const sum = dlg.getByTestId("export-summary");
    await expect(sum).toContainText(tr("de", "line.WS"));
    const box = await sum.boundingBox();
    expect(box && box.x >= 0 && box.x + box.width <= 600).toBe(true);
    await shot(page, "ux003-export-summary_de_600.png");
  });

  test("wall abutments are named apart, the legend names every code shown and the deductions read as such (UX-013, UX-015)", async ({ app, page }) => {
    const id = await crowdedJob("Vikýř výsledky");
    await page.setViewportSize({ width: 1360, height: 860 });
    await openEditor(app, "Vikýř výsledky");
    const results = page.locator(".results");
    const core = await bridge<any>("calc", { project: await loadProject(id) });
    const abut = results.getByTestId("res-abutments");
    for (const code of ["WV", "WS", "WH"]) {
      if ((core.codes[code] ?? 0) >= 0.005) await expect(abut).toContainText(`${code} ${cs(`line.${code}`)}`);
    }
    const legend = results.getByTestId("res-legend");
    for (const code of await results.locator("td.code").allTextContents()) await expect(legend).toContainText(`${code.trim()} `);
    await expect(legend).toContainText(`W ${cs("res.abutment")}`);
    await expect(results.getByTestId("res-deducted")).toContainText(cs("res.obj.netOf", { a: "" }).trim());
    await expect(results.getByTestId("res-deducted")).not.toContainText("−");
    await shot(page, "ux013-ux015-results_cs_1360.png");
  });

  test("the More row says which eave height the roof is calculated from (UX-008)", async ({ app, page }) => {
    await createExample("hip", "Valba okap");
    await openEditor(app, "Valba okap");
    await selectWall(page, 0);
    const def = bar(page, "def.title");
    await expect(def.getByTestId("eave-note")).toHaveText(cs("edge.eaveHeightNote"));
    await expect(def.getByTestId("eave-note")).toBeVisible();
    await expect(def.getByTestId("eave-height")).toHaveAttribute("aria-describedby", "def-eave-note");
    await shot(page, "ux008-eave-note_cs.png");
  });

  test("an AI pitch change shows the previous roof lines dashed (UX-017)", async ({ app, page, mcp }) => {
    const id = await createExample("hip", "Valba AI");
    await page.setViewportSize({ width: 1360, height: 860 });
    await openEditor(app, "Valba AI");
    await expect(page.locator(".ghost line")).toHaveCount(0);
    await mcp.call("update_edges", { id, changes: [{ index: 0, pitch_deg: 25 }, { index: 2, pitch_deg: 25 }], summary: "Sklon podle řezu" });
    const review = page.locator(".review");
    await expect(review).toBeVisible();
    await expect.poll(() => page.locator(".ghost line").count()).toBeGreaterThan(0);
    await shot(page, "ux017-ai-previous-roof_cs_1360.png");
    const before = review.getByRole("button", { name: cs("review.before") });
    await before.click();
    await expect(page.locator(".ghost line")).toHaveCount(0);
    await before.click();
    await expect.poll(() => page.locator(".ghost line").count()).toBeGreaterThan(0);
  });

  test("the join note names the other building without declension (UX-020)", async ({ app, page }) => {
    const id = await createExample("gable", "Dvě budovy");
    const p = await loadProject(id);
    const rect = (x0: number, y0: number, x1: number, y1: number, name: string) => ({
      name,
      edges: [edge(x0, y0, 40), edge(x1, y0, 40), edge(x1, y1, 40), edge(x0, y1, 40)],
    });
    p.parts = [rect(0, 0, 12, 8, "Budova 1"), rect(11, 2, 17, 6, "Budova 2")];
    await bridge("save_project", { id, project: p });
    await page.setViewportSize({ width: 1360, height: 860 });
    await openEditor(app, "Dvě budovy");
    const results = page.locator(".results");
    await expect(results).toContainText("napojeno: Budova 2");
    await expect(results).not.toContainText("napojeno na");
    await shot(page, "ux020-joined_cs_1360.png");
  });

  test("German names Undo and the AI's Revert differently (UX-019)", async ({ app, page, mcp }) => {
    const id = await createExample("hip", "Walm KI");
    await app.open({ locale: "de" });
    await app.openJob("Walm KI");
    await mcp.call("update_edges", { id, changes: [{ index: 0, pitch_deg: 30 }], summary: "Neigung laut Schnitt" });
    const review = page.locator(".review");
    await expect(review.getByRole("button", { name: tr("de", "ai.revert") })).toBeVisible();
    expect(tr("de", "ai.revert")).not.toBe(tr("de", "editor.undo"));
    await expect(review).not.toContainText(tr("de", "editor.undo"));
    await shot(page, "ux019-revert_de.png");
  });
});
