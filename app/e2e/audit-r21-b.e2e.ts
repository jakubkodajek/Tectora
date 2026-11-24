// Guards for the round-21 audit (2026-11-24), half B.
//
// OPENFAIL  a job opened while its review cannot be read: the retry's
//           review brings the banner and the AI tab; when it fails too, a
//           notice says the review could not be read
// UI-2002   a job of hand-drawn faces only with a warning marks the Results tab
// HUNT-2002 two hand-drawn faces on each other are warned about in Results
// UX-2004   Postup of a job of hand-drawn faces only has no empty line heading
// TEST-2009 U20: one hand-drawn face alone is listed under the areas
// UI-2001   the wall bar's tip with no wall selected wraps, never cut mid-word
import type { Page } from "@playwright/test";
import { bridge, createExample, expect, loadProject, test } from "./fixtures";
import { cs, LOCALES, tr } from "./i18n";

// TECTORA_SHOTS=<folder>/ saves a screenshot after each test (fix evidence).
test.afterEach(async ({ page }, info) => {
  const dir = process.env.TECTORA_SHOTS;
  if (!dir) return;
  const id = info.title.match(/(HUNT|UX|UI|ERR|TEST|ARCH|OPENFAIL)-?\d*/)?.[0] ?? "r21";
  const n = info.title.replace(/[^a-z0-9]+/gi, "-").slice(0, 40);
  await page.screenshot({ path: `${dir}r21-${id}-${n}-${process.env.TECTORA_SHOTS_TAG ?? "after"}.png`, timeout: 10_000 }).catch(() => {});
});

const review = (page: Page) => page.locator(".review");
const dockTab = (page: Page, key: string) => page.locator("aside .tabs").getByRole("tab", { name: cs(key) });

/** Answers `review_status` with an error `n` times (every time for Infinity). */
async function failReview(page: Page, n: number) {
  let failed = 0;
  await page.route("**/review_status", async (route) => {
    if (failed < n) {
      failed++;
      await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ err: { code: "io", detail: "read failed" } }) });
      return;
    }
    await route.continue();
  });
  return () => failed;
}

test.describe("round 21: the review read fails when the job opens (OPENFAIL)", () => {
  test("a review read that fails once at the open: the retry brings the banner and the AI tab (OPENFAIL)", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "R21 otevření");
    await mcp.call("update_job", { id, notes: "Od AI", summary: "Poznámka od AI" });
    await app.open({ prefs: { followAi: false } });
    const failed = await failReview(page, 1);
    await app.openJob("R21 otevření");
    await expect.poll(failed).toBe(1);
    await expect(review(page)).toContainText("Poznámka od AI", { timeout: 10_000 });
    // The open's own step after the review: the dock goes to the AI tab.
    await expect(dockTab(page, "dock.ai")).toHaveAttribute("aria-selected", "true");
    await expect(page.locator(".toast", { hasText: cs("review.unread") })).toHaveCount(0);
    app.errors.length = 0;
  });

  test("a review that cannot be read at all is said, not shown as nothing waiting (OPENFAIL)", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "R21 nečitelná kontrola");
    await mcp.call("update_job", { id, notes: "Od AI", summary: "Poznámka od AI" });
    await app.open({ prefs: { followAi: false } });
    const failed = await failReview(page, Infinity);
    await app.openJob("R21 nečitelná kontrola");
    await expect(page.locator(".toast")).toContainText(cs("review.unread"), { timeout: 10_000 });
    expect(failed()).toBeGreaterThanOrEqual(2);
    await expect(review(page)).toHaveCount(0);
    // Readable again: the next read (an AI save) shows the banner.
    await page.unroute("**/review_status");
    await mcp.call("update_job", { id, customer: "AI", summary: "Zákazník od AI" });
    await expect(review(page)).toContainText("Zákazník od AI", { timeout: 10_000 });
    app.errors.length = 0;
  });
});

type Face = { points: { x: number; y: number }[]; pitch_deg: number; label: string };
const square = (x: number, side: number): Face => ({
  points: [{ x, y: 0 }, { x: x + side, y: 0 }, { x: x + side, y: side }, { x, y: side }],
  pitch_deg: 30,
  label: "",
});

/** A job of hand-drawn faces only, opened in the editor. */
async function facesOnly(app: any, name: string, faces: Face[]) {
  const id = await createExample("gable", name);
  const p = await loadProject(id);
  p.parts = [];
  p.dormers = [];
  p.windows = [];
  p.chimneys = [];
  p.roof_openings = [];
  p.manual_faces = faces;
  await bridge("save_project", { id, project: p });
  await app.open();
  await app.openJob(name);
  return id;
}

test.describe("round 21: a job of hand-drawn faces only", () => {
  test("two faces on each other: Results warns and the tab is marked (HUNT-2002, UI-2002)", async ({ app, page }) => {
    await facesOnly(app, "R21 plochy přes sebe", [square(0, 10), square(2, 6)]);
    await page.locator("aside .tabs").getByRole("tab", { name: cs("dock.results") }).click();
    const res = page.locator(".results");
    await expect(res.locator(".warn")).toContainText(cs("res.warn.manual_faces_overlap", { a: 1, b: 2, v: "36,00" }));
    // The job-level warning marks the tab, also while another tab is open.
    await dockTab(page, "dock.job").click();
    await expect(page.getByTestId("results-warned")).toHaveCount(1);
    await expect(page.locator("aside .tabs").getByRole("tab", { name: cs("dock.resultsWarned") })).toHaveCount(1);
  });

  test("one face alone: listed under the areas, no warning mark, and Postup has no empty line heading (TEST-2009 U20, UX-2004)", async ({ app, page }) => {
    await facesOnly(app, "R21 jedna plocha", [square(0, 4)]);
    await page.locator("aside .tabs").getByRole("tab", { name: cs("dock.results") }).click();
    const res = page.locator(".results");
    await expect(res.getByRole("heading", { name: cs("res.areas") })).toHaveCount(1);
    await expect(res).toContainText(cs("manual.faceN", { n: 1 }));
    await expect(page.getByTestId("results-warned")).toHaveCount(0);
    await dockTab(page, "dock.breakdown").click();
    const panel = page.getByRole("tabpanel");
    await expect(panel.getByRole("heading", { name: cs("bd.total") })).toHaveCount(1);
    await expect(panel.getByRole("heading", { name: cs("res.lines") })).toHaveCount(0);
  });
});

test.describe("round 21, UI-2001: the wall bar's tip at the reference size", () => {
  for (const locale of LOCALES) {
    test(`the tip with no wall selected is shown whole at 1360x860 in ${locale} (UI-2001)`, async ({ app, page }) => {
      await createExample("gable", `R21 tip ${locale}`);
      await app.open({ locale });
      await app.openJob(`R21 tip ${locale}`);
      const tip = page.getByTestId("def-tip");
      await expect(tip).toHaveText(tr(locale, "editor.tip"));
      const fit = await tip.evaluate((p) => {
        const row = p.parentElement!;
        return { tip: p.scrollWidth - p.clientWidth, row: row.scrollWidth - row.clientWidth, h: p.clientHeight, bottom: row.getBoundingClientRect().bottom - p.getBoundingClientRect().bottom };
      });
      // Nothing runs out of the tip or the strip: the text wraps inside it.
      expect(fit.tip).toBeLessThanOrEqual(0);
      expect(fit.row).toBeLessThanOrEqual(1);
      expect(fit.bottom).toBeGreaterThanOrEqual(-1);
    });
  }
});
