// Guards for the round-19 audit (2026-11-24), design A: the review by the
// job's version.
//
// ERR-1801  one Accept ends the review, also when the AI put back a change
//           the user reverted (Accept marks the newest entry shown)
// ARCH-1802 the AI's put-back and its own take-out leave the job as the
//           user left it: no banner
import type { Page } from "@playwright/test";
import { openEditor } from "./editor";
import { bridge, createExample, expect, test, waitForProject } from "./fixtures";
import { cs } from "./i18n";

// TECTORA_SHOTS=<folder>/ saves a screenshot after each test (fix evidence).
test.afterEach(async ({ page }, info) => {
  const dir = process.env.TECTORA_SHOTS;
  if (!dir) return;
  const id = info.title.match(/(HUNT|UX|UI|ERR|TEST|ARCH)-\d+/)?.[0] ?? "r19";
  await page.screenshot({ path: `${dir}r19-${id}-${process.env.TECTORA_SHOTS_TAG ?? "after"}.png`, timeout: 10_000 }).catch(() => {});
});

const review = (page: Page) => page.locator(".review");
const errorToasts = (page: Page) => page.locator(".stack .toast.error");

type Logged = { ts_ms: number; actor: string; summary: string };
const newest = async (id: string) => (await bridge<Logged[]>("project_activity", { id }))[0];

test.describe("round 19: the review by the job's version", () => {
  test("one Accept ends the review after the AI put back a change the user reverted (ERR-1801)", async ({ app, page, mcp }) => {
    const name = "R19 přijmout";
    const id = await createExample("gable", name);
    await openEditor(app, name);
    await mcp.call("update_job", { id, notes: "Od AI", summary: "Poznámka od AI" });
    await expect(review(page)).toContainText("Poznámka od AI");
    await review(page).getByRole("button", { name: cs("review.accept") }).click();
    await expect(review(page)).toHaveCount(0);
    await mcp.call("update_job", { id, customer: "AI Zákazník", summary: "Zákazník od AI" });
    await expect(review(page)).toContainText("Zákazník od AI");
    await review(page).getByRole("button", { name: cs("ai.revert"), exact: true }).click();
    await waitForProject(id, (p) => !p.job.customer);
    await expect(review(page)).toHaveCount(0);
    const r = await newest(id);
    await mcp.call("revert_to", { id, ts_ms: r.ts_ms, summary: "Vracím zákazníka." });
    await waitForProject(id, (p) => p.job.customer === "AI Zákazník");
    await expect(review(page)).toBeVisible();
    // The editor has shown the AI's put-back: one Accept covers it.
    await expect.poll(async () => (await newest(id)).summary).toContain("Vracím zákazníka.");
    await review(page).getByRole("button", { name: cs("review.accept") }).click();
    await expect(review(page)).toHaveCount(0);
    expect((await mcp.call("get_review", { id })).pending).toBeNull();
    // It stays ended after the history is read again.
    await page.waitForTimeout(300);
    await expect(review(page)).toHaveCount(0);
    await expect(errorToasts(page)).toHaveCount(0);
  });

  test("the AI's put-back of what the user reverted and its own take-out of it leave no banner (ARCH-1802)", async ({ app, page, mcp }) => {
    const name = "R19 tam a zpět";
    const id = await createExample("gable", name);
    await openEditor(app, name);
    await mcp.call("update_job", { id, notes: "Od AI", summary: "Poznámka od AI" });
    await expect(review(page)).toContainText("Poznámka od AI");
    await review(page).getByRole("button", { name: cs("review.accept") }).click();
    await expect(review(page)).toHaveCount(0);
    const a1 = await newest(id);
    await bridge("revert_last_change", { id, expectTs: a1.ts_ms });
    await waitForProject(id, (p) => !p.notes);
    const r = await newest(id);
    await mcp.call("revert_to", { id, ts_ms: r.ts_ms, summary: "Vracím poznámku." });
    await waitForProject(id, (p) => p.notes === "Od AI");
    await expect(review(page)).toBeVisible();
    const p = await newest(id);
    await mcp.call("revert_to", { id, ts_ms: p.ts_ms, summary: "Přece ne." });
    await waitForProject(id, (q) => !q.notes);
    await expect(review(page)).toHaveCount(0);
    expect((await mcp.call("get_review", { id })).pending).toBeNull();
    await expect(errorToasts(page)).toHaveCount(0);
  });
});
