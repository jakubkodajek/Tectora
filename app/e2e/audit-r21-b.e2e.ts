// Guards for the round-21 audit (2026-11-24), half B.
//
// OPENFAIL  a job opened while its review cannot be read: the retry's
//           review brings the banner and the AI tab; when it fails too, a
//           notice says the review could not be read
import type { Page } from "@playwright/test";
import { createExample, expect, test } from "./fixtures";
import { cs } from "./i18n";

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
