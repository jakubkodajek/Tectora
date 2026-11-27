// Audit 2026-11-24 round 21, half A: the review's banner line is the net
// difference against the approved state (UX-2001, owner decision 4), and
// the library marks and filters the jobs whose AI change waits for review
// (UX-2003, owner decision 3).
import type { Page } from "@playwright/test";
import { bridge, createExample, expect, loadProject, test, waitForProject } from "./fixtures";
import { cs } from "./i18n";

const review = (page: Page) => page.locator(".review");
const row = (page: Page, name: string) => page.getByRole("row").filter({ hasText: name });

test.describe("round 21: review banner and library", () => {
  test("the library marks the jobs waiting for review and filters them", async ({ app, page, mcp }) => {
    const waits = await createExample("gable", "R21 čeká");
    await mcp.call("update_job", { id: waits, notes: "Od AI", summary: "Poznámka od AI" });
    const done = await createExample("gable", "R21 přijato");
    await mcp.call("update_job", { id: done, notes: "Od AI", summary: "Poznámka od AI" });
    const log = await bridge<{ ts_ms: number }[]>("project_activity", { id: done });
    await bridge("accept_changes", { id: done, tsMs: log[log.length - 1].ts_ms });
    await createExample("gable", "R21 moje");

    await app.open({ prefs: { followAi: false, libraryView: "list" } });
    await expect(row(page, "R21 čeká").getByTestId("review-mark")).toHaveText(cs("lib.waitsReview"));
    // Both were changed by the AI; only one still waits.
    await expect(row(page, "R21 přijato").getByTestId("ai-tag")).toBeVisible();
    await expect(row(page, "R21 přijato").getByTestId("review-mark")).toHaveCount(0);
    await expect(row(page, "R21 moje").getByTestId("review-mark")).toHaveCount(0);
    await row(page, "R21 čeká").click();
    await expect(page.getByTestId("detail-review-mark")).toHaveText(cs("lib.waitsReview"));

    const filter = page.getByTestId("lib-review");
    await expect(filter).toContainText("1");
    await filter.click();
    await expect(row(page, "R21 čeká")).toHaveCount(1);
    await expect(row(page, "R21 přijato")).toHaveCount(0);
    await expect(row(page, "R21 moje")).toHaveCount(0);

    // Accept in the editor: the mark and the count go.
    await app.openJob("R21 čeká");
    await review(page).getByRole("button", { name: cs("review.accept") }).click();
    await expect(review(page)).toHaveCount(0);
    await page.getByRole("button", { name: cs("editor.back") }).first().click();
    await expect(page.getByTestId("lib-review")).toContainText("0");
  });

  test("the banner names the job's value, not the AI's, after the user's own edit", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "R21 rozdíl");
    await mcp.call("update_edges", { id, changes: [{ index: 0, overhang: 0.9 }], summary: "Přesah podle řezu" });
    const cur = await loadProject(id);
    const mine = structuredClone(cur);
    mine.parts[0].edges[0].overhang = 0.7;
    await bridge("save_project_checked", { id, project: mine, base: cur });
    await waitForProject(id, (p) => p.parts[0].edges[0].overhang === 0.7);
    await app.open({ prefs: { followAi: false } });
    await app.openJob("R21 rozdíl");
    const line = review(page).locator(".review-list");
    await expect(line).toContainText("0,70");
    await expect(line).not.toContainText("0,90");
  });
});
