// Guards for the round-15 audit (2026-11-23), part A: what an entry is.
//
// ARCH-1401  an AI save summarised like Tectora's own revert sentence is a
//            change: the banner and the history show the AI's words in
//            quotation marks, and the history's Revert undoes that save
// HUNT-1401  Revert all counts only the AI's steps against the banner: a
//            user's revert written by a second Tectora on the same folder
//            while the question was open is no "AI step reverted too"
import type { Page } from "@playwright/test";
import { openEditor } from "./editor";
import { bridge, createExample, expect, loadProject, noToast, test, waitForProject } from "./fixtures";
import { cs } from "./i18n";

// TECTORA_SHOTS=<folder>/ saves a screenshot after each test (fix evidence).
test.afterEach(async ({ page }, info) => {
  const dir = process.env.TECTORA_SHOTS;
  if (!dir) return;
  const id = info.title.match(/(HUNT|UX|UI|ERR|TEST|ARCH)-\d+/)?.[0] ?? "r15";
  const n = info.title.replace(/[^a-z0-9]+/gi, "-").slice(0, 40);
  await page.screenshot({ path: `${dir}r15-${id}-${n}-${process.env.TECTORA_SHOTS_TAG ?? "after"}.png`, timeout: 10_000 }).catch(() => {});
});

const review = (page: Page) => page.locator(".review");
const dockTab = (page: Page, key: string) => page.locator("aside .tabs").getByRole("tab", { name: cs(key) });
const entries = (page: Page) => page.getByTestId("history-entry");
const pitches = async (id: string) => (await loadProject(id)).parts[0].edges.map((e: { pitch_deg: number }) => e.pitch_deg).join(",");

test.describe("round 15, part A: an entry is what its fields say", () => {
  test("an AI save summarised as a revert is shown as the AI's quoted words, and the history's Revert undoes that save (ARCH-1401)", async ({ app, page, mcp }) => {
    const name = "R15 přestrojení";
    const id = await createExample("gable", name);
    await mcp.call("update_edges", { id, changes: [{ index: 0, pitch_deg: 45 }], summary: "Wall 1 pitch 45" });
    // A save that changes two walls, worded as the revert of the first step.
    await mcp.call("update_edges", { id, changes: [{ index: 0, pitch_deg: 40 }, { index: 2, pitch_deg: 20 }], summary: "Reverted: Wall 1 pitch 45" });
    await mcp.call("update_job", { id, customer: "Evil s.r.o.", summary: "Reverted: Customer" });
    expect(await pitches(id)).toBe("40,90,20,90");
    await app.open({ prefs: { followAi: false } });
    await app.openJob(name);
    await expect(review(page)).toContainText(cs("review.steps", { n: 3 }));
    const line = review(page).locator(".review-summary");
    await expect(line).toHaveText("„Wall 1 pitch 45“ → „Reverted: Wall 1 pitch 45“ → „Reverted: Customer“");
    await expect(line).not.toContainText(cs("chg.revertedEntry", { s: "Customer" }));
    await dockTab(page, "dock.ai").click();
    await expect(entries(page).first().locator(".summary")).toHaveText("„Reverted: Customer“");
    await entries(page).first().getByRole("button", { name: cs("ai.revert") }).click();
    const p = await waitForProject(id, (p) => p.job.customer === "");
    // Only that save: the two walls the step before it set stay.
    expect(p.parts[0].edges.map((e: { pitch_deg: number }) => e.pitch_deg).join(",")).toBe("40,90,20,90");
    // The one AI change undone, named in its quoted words (UX-1603; was "AI changes reverted").
    await expect(entries(page).first().locator(".summary")).toHaveText(cs("ai.revertedOne", { s: "„Reverted: Customer“" }));
  });

  test("Revert all says nothing more when a second Tectora reverted one of the AI steps while it asked (HUNT-1401)", async ({ app, page, mcp }) => {
    const name = "R15 kolega";
    const id = await createExample("gable", name);
    await openEditor(app, name);
    await mcp.call("update_job", { id, notes: "Od AI", summary: "Poznámka od AI" });
    await expect(review(page)).toContainText("Poznámka od AI");
    await mcp.call("update_job", { id, customer: "AI Zákazník", summary: "Zákazník od AI" });
    await expect(review(page)).toContainText(cs("review.steps", { n: 2 }));
    // The question stays open until the test answers it.
    await page.evaluate(() => {
      const w = window as any;
      w.confirm = (m: string) => {
        w.__e2e.dialogs.push({ kind: "confirm", message: String(m) });
        return new Promise((res) => (w.__answer = res));
      };
    });
    await review(page).getByRole("button", { name: cs("review.revertAll") }).click();
    await expect.poll(() => page.evaluate(() => typeof (window as any).__answer)).toBe("function");
    // A colleague's Tectora on the same folder reverts the latest AI step (actor user).
    await bridge("revert_last_change", { id, expectTs: null });
    await page.evaluate(() => (window as any).__answer(true));
    const p = await waitForProject(id, (p) => !p.notes && p.job.customer === "");
    expect(p.notes).toBe("");
    await expect(review(page)).toHaveCount(0);
    await noToast(app.toasts().filter({ hasText: cs("review.revertedMore", { n: 1 }) }), 2000, "an AI step reported reverted that never came in");
    // The history line counts the two changes, not the colleague's revert.
    const log = await bridge<{ actor: string; summary: string }[]>("project_activity", { id });
    expect(log[0].summary).toBe("Reverted 2 changes since: Poznámka od AI");
  });
});
