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
// TEST-2010 (round-20 TEST-1906's UI half): a face whose check answers after
//           a corner was added is not added (Z09). The Z10 guard (a Revert
//           out when the check answers) cannot be reached from the UI: the
//           banner's Revert waits for the check (busy_edits), and the editor
//           is inert while a Revert is out.
// TEST-2007 an older answer of the trash listing or of a job's files never
//           replaces a newer one (held-answer tests of the round-20 guards)
import type { Page } from "@playwright/test";
import { bridge, createExample, expect, loadProject, scratchFile, test } from "./fixtures";
import { clickPlan, openEditor, tool } from "./editor";
import { cs, LOCALES, tr } from "./i18n";
import { hold, timeline } from "./review";

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

test.describe("round 21, TEST-2010: a face whose check answers late", () => {
  const isCheck = (b: any) => b?.request?.op === "check_face";
  const frame = (page: Page) => page.evaluate(() => new Promise((r) => requestAnimationFrame(() => setTimeout(r, 50))));

  test("a corner added while the face's check is out: the face is not closed, the draft goes on (TEST-2010, Z09)", async ({ app, page }) => {
    const id = await createExample("gable", "R21 pozdní kontrola");
    await openEditor(app, "R21 pozdní kontrola");
    await app.saved();
    await tool(page, "tool.face").click();
    for (const [x, y] of [[2, 1], [6, 1], [6, 3]]) await clickPlan(page, x, y);
    const points = page.getByTestId("place-points");
    await expect(points).toHaveText(cs("place.points", { n: 3 }));
    const held = await hold(page, "item_geometry", isCheck);
    const checks = timeline(page, "item_geometry");
    await page.keyboard.press("Enter");
    await expect.poll(() => held.held()).toBe(1);
    await clickPlan(page, 2, 3);
    await expect(points).toHaveText(cs("place.points", { n: 4 }));
    held.release();
    await expect.poll(() => checks.answered.length).toBeGreaterThanOrEqual(1);
    await frame(page);
    // The stale close added nothing: the four corners are still being placed.
    await expect(points).toHaveText(cs("place.points", { n: 4 }));
    await app.saved();
    expect((await loadProject(id)).manual_faces ?? []).toHaveLength(0);
  });
});

test.describe("round 21, TEST-2007: older answers of the trash and the files", () => {
  /** Holds only the first answer to `cmd` after the call. */
  const holdFirst = (page: Page, cmd: string) => {
    let armed = true;
    return hold(page, cmd, () => {
      if (!armed) return false;
      armed = false;
      return true;
    });
  };

  test("the trash keeps the newest listing when an older one answers last (TEST-2007, U03)", async ({ app, page, mcp }) => {
    const a = await createExample("gable", "R21 koš A");
    const b = await createExample("hip", "R21 koš B");
    await app.open({ prefs: { followAi: false } });
    await page.getByTestId("lib-trash").click();
    const list = () => page.getByTestId("trash-list");
    const held = await holdFirst(page, "list_deleted");
    const reads = timeline(page, "list_deleted");
    await mcp.call("delete_project", { id: a, summary: "Smazat A" });
    await expect.poll(() => held.held(), { timeout: 10_000 }).toBe(1);
    // A newer listing (both deleted) is shown while the older one is held.
    await mcp.call("delete_project", { id: b, summary: "Smazat B" });
    await expect(list()).toContainText("R21 koš B", { timeout: 10_000 });
    await expect(list()).toContainText("R21 koš A");
    held.release();
    await expect.poll(() => reads.answered.length >= reads.sent.length, { timeout: 10_000 }).toBe(true);
    await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => setTimeout(r, 50))));
    await expect(list()).toContainText("R21 koš B");
    await expect(page.getByTestId("lib-trash")).toContainText("2");
  });

  test("a job's files keep the newest listing when an older one answers last (TEST-2007, U04)", async ({ app, page }) => {
    const id = await createExample("gable", "R21 soubory");
    await bridge("attach_files", { id, paths: [scratchFile("a.pdf", "%PDF"), scratchFile("b.pdf", "%PDF")] });
    await openEditor(app, "R21 soubory");
    await page.locator("aside .tabs").getByRole("tab", { name: cs("dock.job") }).click();
    const files = page.locator("aside .job ul.files li");
    await expect(files).toHaveCount(2);
    await app.answerConfirm(true);
    // The listing after removing a.pdf is held (it lists b.pdf) ...
    const held = await holdFirst(page, "list_attachments");
    const reads = timeline(page, "list_attachments");
    await files.filter({ hasText: "a.pdf" }).getByRole("button", { name: cs("files.remove") }).click();
    await expect.poll(() => held.held(), { timeout: 10_000 }).toBe(1);
    // ... and the one after removing b.pdf too answers first: no files.
    await files.filter({ hasText: "b.pdf" }).getByRole("button", { name: cs("files.remove") }).click();
    await expect(files).toHaveCount(0, { timeout: 10_000 });
    held.release();
    await expect.poll(() => reads.answered.length >= reads.sent.length, { timeout: 10_000 }).toBe(true);
    await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => setTimeout(r, 50))));
    await expect(files).toHaveCount(0);
    expect(await bridge<unknown[]>("list_attachments", { id })).toEqual([]);
  });
});
