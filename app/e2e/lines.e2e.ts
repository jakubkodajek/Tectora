// Line lengths by line code in the Results tab (TEST-007): every row (T eave,
// F ridge, G hip, K valley, O verge, P shed top edge, c pitch break, …) shows
// the core's figure for that code for the saved file, both the calculation's
// totals and the per-code list the stored result keeps for the offer. A
// swapped mapping (hip and valley, eave and verge) fails here.
import type { Page } from "@playwright/test";
import { bar, openEditor, selectWall, setField } from "./editor";
import { bridge, createExample, expect, loadProject, test, waitForProject } from "./fixtures";
import { cs } from "./i18n";

/** The calculation's total behind each line code of the results table. */
const TOTAL_BY_CODE: Record<string, string> = {
  T: "eave_length",
  F: "ridge_length",
  G: "hip_length",
  K: "valley_length",
  O: "verge_length",
  P: "pult_length",
  c: "knick_length",
  S: "gorge_length",
  W: "abutment_length",
};

/**
 * The row a code of the core's per-code list belongs to: verges are listed
 * by side (OL, OR, OS), every code without a row of its own (abutments WV,
 * WS, …) adds to W (as `quantities::subtract_minus_lines`).
 */
const listGroup = (code: string) => (code.startsWith("O") ? "O" : code in TOTAL_BY_CODE ? code : "W");

const parse =(s: string) => Number(s.replace("m", "").replace(/−/g, "-").replace(/\s/g, "").replace(",", "."));

/** The lines table of the Results tab as code → shown metres. */
async function shownLines(page: Page): Promise<Record<string, number>> {
  const rows = page.locator(".results tr").filter({ has: page.locator("td.code") });
  const out: Record<string, number> = {};
  for (const row of await rows.all()) {
    const code = ((await row.locator("td.code").textContent()) ?? "").trim();
    out[code] = parse((await row.locator("td.v").textContent()) ?? "");
  }
  return out;
}

/** What the core says for the saved file: totals and the per-code list. */
async function coreLines(id: string) {
  const project = await loadProject(id);
  const calc = await bridge<any>("calc", { project });
  const snap = await bridge<any>("take_snapshot", { project });
  return { totals: calc.totals as Record<string, number>, byCode: snap.lines_by_code as Record<string, number> };
}

/** Differences between the table and the core, empty when they agree to the cent. */
async function mismatches(page: Page, id: string): Promise<string[]> {
  const shown = await shownLines(page);
  const { totals, byCode } = await coreLines(id);
  const bad: string[] = [];
  for (const [code, key] of Object.entries(TOTAL_BY_CODE)) {
    const want = totals[key] ?? 0;
    // P, c, S and W are listed only when the roof has them.
    if (!(code in shown)) {
      if (want >= 0.005 || ["T", "F", "G", "K", "O"].includes(code)) bad.push(`${code}: row missing, core ${want}`);
      continue;
    }
    if (!(Math.abs(shown[code] - want) < 0.006)) bad.push(`${code}: shown ${shown[code]}, core ${key} ${want}`);
    const listed = Object.entries(byCode)
      .filter(([k]) => listGroup(k) === code)
      .reduce((s, [, v]) => s + v, 0);
    // Each listed code is rounded to the cent on its own.
    if (!(Math.abs(shown[code] - listed) < 0.02)) bad.push(`${code}: shown ${shown[code]}, core list ${listed}`);
  }
  for (const code of Object.keys(shown)) if (!(code in TOTAL_BY_CODE)) bad.push(`${code}: unknown code in the table`);
  return bad;
}

async function expectCoreLines(page: Page, id: string, codes: string[]) {
  await expect(page.locator(".results .hero strong")).not.toHaveText("—");
  await expect.poll(() => mismatches(page, id), { timeout: 8000 }).toEqual([]);
  // The roof really has these lines, so the comparison is not 0 = 0.
  const shown = await shownLines(page);
  for (const c of codes) expect(shown[c], `${c} > 0`).toBeGreaterThan(0);
}

test.describe("line lengths by code", () => {
  test("a hip roof shows the core's eave, ridge and hip lengths", async ({ app, page }) => {
    const id = await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    await expectCoreLines(page, id, ["T", "F", "G"]);
  });

  test("an L-shaped roof shows its valleys under K", async ({ app, page }) => {
    const id = await createExample("l-shape", "Elko");
    await openEditor(app, "Elko");
    await expectCoreLines(page, id, ["T", "F", "G", "K"]);
  });

  test("a gable roof shows its verges under O", async ({ app, page }) => {
    const id = await createExample("gable", "Sedlo");
    await openEditor(app, "Sedlo");
    await expectCoreLines(page, id, ["T", "F", "O"]);
  });

  test("a shed roof shows its top edge under P", async ({ app, page }) => {
    const id = await createExample("shed", "Pult");
    await openEditor(app, "Pult");
    await expectCoreLines(page, id, ["T", "P"]);
  });

  test("a roof with pitch breaks shows them under c", async ({ app, page }) => {
    const id = await createExample("gable", "Mansarda");
    const p = await loadProject(id);
    for (const i of [0, 2]) {
      const e = p.parts[0].edges[i];
      e.pitch_deg = 60;
      e.breaks = [{ height: e.eave_height + 1.5, pitch_deg: 30 }];
    }
    await bridge("save_project", { id, project: p });
    await openEditor(app, "Mansarda");
    await expectCoreLines(page, id, ["T", "F", "O", "c"]);
  });

  test("the lines follow an edit in the editor", async ({ app, page }) => {
    const id = await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    await expectCoreLines(page, id, ["T", "F", "G"]);
    const before = await shownLines(page);
    await selectWall(page, 1);
    await setField(bar(page, "def.title").getByLabel(cs("edge.pitch")), 60);
    await waitForProject(id, (q) => q.parts[0].edges[1].pitch_deg === 60);
    await expect.poll(async () => (await shownLines(page)).F).not.toBe(before.F);
    await expectCoreLines(page, id, ["T", "F", "G"]);
  });
});
