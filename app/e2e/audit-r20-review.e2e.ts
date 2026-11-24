// Audit 2026-11-24 round 20: the review keeps the job as the user last
// settled it (ADR 0007).
import type { Page } from "@playwright/test";
import { bridge, createExample, expect, test, waitForProject } from "./fixtures";
import { cs } from "./i18n";

const review = (page: Page) => page.locator(".review");
const undo = (page: Page) => page.locator("body").press("ControlOrMeta+z");
const footerSave = (page: Page) => page.locator(".statusbar .save");

test.describe("round 20: the review's baseline", () => {
  // UX-1901 / TEST-1901 (t2002 in the editor): A1 accepted, the AI's revert
  // of A1, the banner's Revert, then Ctrl+Z: the banner comes back.
  test("Ctrl+Z after Revert of the AI's revert of accepted work brings the banner back", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "R20 zpět přijaté");
    await mcp.call("update_job", { id, notes: "Od AI", summary: "Poznámka od AI" });
    const log = await bridge<{ ts_ms: number; summary: string }[]>("project_activity", { id });
    await bridge("accept_changes", { id, tsMs: log.find((e) => e.summary === "Poznámka od AI")!.ts_ms });
    await app.open({ prefs: { followAi: false } });
    await app.openJob("R20 zpět přijaté");
    await expect(review(page)).toHaveCount(0);
    await mcp.call("revert_last_change", { id, summary: "Beru poznámku zpět" });
    await waitForProject(id, (p) => !p.notes);
    await expect(review(page)).toContainText("Beru poznámku zpět");
    await app.answerConfirm(true);
    await review(page).getByRole("button", { name: cs("ai.revert"), exact: true }).or(review(page).getByRole("button", { name: cs("review.revertAll") })).first().click();
    await waitForProject(id, (p) => p.notes === "Od AI");
    await expect(review(page)).toHaveCount(0);
    await expect(footerSave(page)).toHaveText(cs("editor.saved"));
    await undo(page);
    await waitForProject(id, (p) => !p.notes);
    await expect(review(page), "the AI's take-out of accepted work waits again").toBeVisible();
    await expect.poll(async () => (await mcp.call("get_review", { id })).pending?.count).toBe(1);
  });

  // ERR-1905's core half: the banner's Revert all of a review accepted
  // meanwhile is refused as stale and takes nothing out.
  test("a stale Revert all of an accepted review writes nothing", async ({ mcp }) => {
    const id = await createExample("gable", "R20 zastaralé");
    await mcp.call("update_job", { id, notes: "Od AI", summary: "Poznámka od AI" });
    const shown = (await bridge<{ since_ts_ms: number; steps: number[] } | null>("review_status", { id }))!;
    await bridge("accept_changes", { id, tsMs: shown.steps[shown.steps.length - 1] });
    const r = await bridge("revert_changes", { id, tsMs: shown.since_ts_ms, seenTs: null, review: true }).then(
      () => "written",
      (e: Error) => e.message,
    );
    expect(r).toContain("revert_stale");
    await waitForProject(id, (p) => p.notes === "Od AI");
  });

  // UX-1905 (owner decision 2): Duplicate of a job with a waiting AI change
  // shows the same banner on the copy.
  test("a duplicate carries the waiting review", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "R20 kopie");
    await mcp.call("update_job", { id, notes: "Od AI", summary: "Poznámka od AI" });
    const copy = await bridge<string>("duplicate_project", { id, name: "Duplikát R20", job: null, withFiles: false });
    const status = await bridge<{ count: number } | null>("review_status", { id: copy });
    expect(status?.count).toBe(1);
    await app.open({ prefs: { followAi: false } });
    await app.openJob("Duplikát R20");
    await expect(review(page)).toContainText("Poznámka od AI");
  });
});
