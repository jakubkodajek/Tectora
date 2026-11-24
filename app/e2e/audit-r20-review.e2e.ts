// Audit 2026-11-24 round 20: the review keeps the job as the user last
// settled it (ADR 0007).
import type { Page } from "@playwright/test";
import { bridge, createExample, expect, loadProject, test, waitForProject } from "./fixtures";
import { cs } from "./i18n";
import { timeline } from "./review";

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

  // Audit 2026-11-24 TEST-2001 (ERR-1905, the editor's half; the test
  // reviewer's r21-stale.e2e.ts): the banner of a review that was accepted
  // elsewhere meanwhile, the editor not having heard of it: its Revert is
  // refused by the core (`revert_stale`), the accepted AI change stays, the
  // editor says so and reads the job again.
  for (const steps of [1, 2]) {
    test(`a stale banner's ${steps === 1 ? "Revert" : "Revert all"} takes nothing accepted out and reloads (TEST-2001)`, async ({ app, page, mcp }) => {
      const name = `R21 zastaralý banner ${steps}`;
      const id = await createExample("gable", name);
      await mcp.call("update_job", { id, notes: "Od AI", summary: "Poznámka od AI" });
      if (steps === 2) await mcp.call("update_job", { id, customer: "AI", summary: "Zákazník od AI" });
      await app.open({ prefs: { followAi: false } });
      await app.openJob(name);
      await expect(review(page)).toContainText(steps === 1 ? "Poznámka od AI" : "Zákazník od AI");
      // From now on the editor hears of no file change: the Accept elsewhere stays unknown to it.
      await page.route("**/__events", (route) => route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ ok: [] }) }));
      await page.waitForResponse((r) => r.url().includes("__events"));
      const shown = (await bridge<{ steps: number[] } | null>("review_status", { id }))!;
      expect(shown.steps).toHaveLength(steps);
      await bridge("accept_changes", { id, tsMs: shown.steps[shown.steps.length - 1] });
      const loads = timeline(page, "load_project");
      await app.answerConfirm(true);
      const button = steps === 1 ? review(page).getByRole("button", { name: cs("ai.revert"), exact: true }) : review(page).getByRole("button", { name: cs("review.revertAll") });
      await button.click();
      await expect(page.locator(".toast")).toContainText(cs("err.review_stale"), { timeout: 10_000 });
      await expect.poll(() => loads.sent.length, { timeout: 10_000 }).toBeGreaterThan(0);
      await expect(review(page)).toHaveCount(0);
      const saved = await loadProject(id);
      expect(saved.notes).toBe("Od AI");
      if (steps === 2) expect(saved.job.customer).toBe("AI");
      expect(await bridge("review_status", { id })).toBeNull();
      app.errors.length = 0;
    });
  }
});
