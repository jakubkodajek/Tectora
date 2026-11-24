// Guards for the round-17 audit (2026-11-24), design A: one rule for the
// review (the change log says what stands; Undo of a Revert is logged).
//
// UX-1601   Ctrl+Z after Revert all (or a one-step Revert) takes the revert
//           back through the core: the banner comes back with the same
//           steps; Ctrl+Shift+Z takes them out again (owner decision 2)
// UX-1601   Ctrl+Z over an AI's outside save is a logged revert (design A3)
// ERR-1601  the AI's own revert of its only step leaves no banner (owner
//           decision 1)
// HUNT-1601 a job the AI created: Revert all goes back to the job right
//           after the creation, the lone creation offers Delete job (owner
//           decision 3)
import type { Page } from "@playwright/test";
import { openEditor } from "./editor";
import { createExample, expect, noToast, test, waitForProject } from "./fixtures";
import { cs } from "./i18n";

// TECTORA_SHOTS=<folder>/ saves a screenshot after each test (fix evidence).
test.afterEach(async ({ page }, info) => {
  const dir = process.env.TECTORA_SHOTS;
  if (!dir) return;
  const id = info.title.match(/(HUNT|UX|UI|ERR|TEST|ARCH)-\d+/)?.[0] ?? "r17";
  const n = info.title.replace(/[^a-z0-9]+/gi, "-").slice(0, 40);
  await page.screenshot({ path: `${dir}r17-${id}-${n}-${process.env.TECTORA_SHOTS_TAG ?? "after"}.png`, timeout: 10_000 }).catch(() => {});
});

const review = (page: Page) => page.locator(".review");
const dockTab = (page: Page, key: string) => page.locator("aside .tabs").getByRole("tab", { name: cs(key) });
const entries = (page: Page) => page.getByTestId("history-entry");
const revertIn = (page: Page, i: number) => entries(page).nth(i).getByRole("button", { name: cs("ai.revert"), exact: true });
const errorToasts = (page: Page) => page.locator(".stack .toast.error");
const undo = (page: Page) => page.locator("body").press("ControlOrMeta+z");
const redo = (page: Page) => page.locator("body").press("ControlOrMeta+Shift+z");

test.describe("round 17, design A: the review reads what stands in the log", () => {
  test("Ctrl+Z after Revert all brings the review back, Ctrl+Shift+Z takes it out again, both logged (UX-1601)", async ({ app, page, mcp }) => {
    const name = "R17 zpět";
    const id = await createExample("gable", name);
    await openEditor(app, name);
    await mcp.call("update_job", { id, notes: "Od AI", summary: "Poznámka od AI" });
    await expect(review(page)).toContainText("Poznámka od AI");
    await mcp.call("update_job", { id, customer: "AI Zákazník", summary: "Zákazník od AI" });
    await expect(review(page)).toContainText(cs("review.steps", { n: 2 }));
    await app.answerConfirm(true);
    await review(page).getByRole("button", { name: cs("review.revertAll") }).click();
    await waitForProject(id, (p) => !p.notes && p.job.customer === "");
    await expect(review(page)).toHaveCount(0);

    await undo(page);
    await waitForProject(id, (p) => p.notes === "Od AI" && p.job.customer === "AI Zákazník");
    await expect(review(page)).toContainText(cs("review.steps", { n: 2 }));
    await expect(review(page).locator(".review-summary")).toHaveText("„Poznámka od AI“ → „Zákazník od AI“");
    let status = await mcp.call("get_review", { id });
    expect(status.pending?.count).toBe(2);
    await dockTab(page, "dock.ai").click();
    // Newest first: the take-back, the taken-back revert (muted), the two AI steps (standing).
    await expect(entries(page).first().locator(".summary")).toHaveText(cs("ai.revertTakenBack"));
    await expect(entries(page).nth(1).getByTestId("history-undone")).toHaveCount(1);
    await expect(entries(page).nth(2).getByTestId("history-undone")).toHaveCount(0);
    await expect(entries(page).nth(3).getByTestId("history-undone")).toHaveCount(0);
    const history = await mcp.call("get_history", { id });
    expect(history[0].kind).toBe("revert");
    expect(history[0].actor).toBe("user");

    await redo(page);
    await waitForProject(id, (p) => !p.notes && p.job.customer === "");
    await expect(review(page)).toHaveCount(0);
    status = await mcp.call("get_review", { id });
    expect(status.pending).toBeNull();
    expect(await errorToasts(page).count()).toBe(0);
  });

  test("Ctrl+Z after a one-step Revert brings that step back, and the next Revert undoes exactly the row it names (UX-1601)", async ({ app, page, mcp }) => {
    const name = "R17 jeden";
    const id = await createExample("gable", name);
    await openEditor(app, name);
    await mcp.call("update_job", { id, notes: "Od AI", summary: "Poznámka od AI" });
    await mcp.call("update_job", { id, customer: "AI Zákazník", summary: "Zákazník od AI" });
    await expect(review(page)).toContainText(cs("review.steps", { n: 2 }));
    await dockTab(page, "dock.ai").click();
    await revertIn(page, 0).click();
    await waitForProject(id, (p) => p.job.customer === "" && p.notes === "Od AI");
    // The history names the one step it undid (UX-1603).
    await expect(entries(page).first().locator(".summary")).toHaveText(cs("ai.revertedOne", { s: "„Zákazník od AI“" }));
    await expect(review(page)).not.toContainText(cs("review.steps", { n: 2 }));

    await undo(page);
    await waitForProject(id, (p) => p.job.customer === "AI Zákazník" && p.notes === "Od AI");
    await expect(review(page)).toContainText(cs("review.steps", { n: 2 }));
    // Revert is on the newest standing AI step again, the customer, and undoes only it.
    const row = entries(page).filter({ hasText: "„Zákazník od AI“" }).filter({ has: page.getByRole("button", { name: cs("ai.revert"), exact: true }) });
    await expect(row).toHaveCount(1);
    await row.getByRole("button", { name: cs("ai.revert"), exact: true }).click();
    const p = await waitForProject(id, (p) => p.job.customer === "");
    expect(p.notes).toBe("Od AI");
    await expect(review(page)).toContainText("Poznámka od AI");
    expect(await errorToasts(page).count()).toBe(0);
  });

  test("Ctrl+Z over an AI's outside save is a logged revert, and the review follows (UX-1601)", async ({ app, page, mcp }) => {
    const name = "R17 venku";
    const id = await createExample("gable", name);
    await openEditor(app, name);
    await mcp.call("update_job", { id, notes: "Od AI", summary: "Poznámka od AI" });
    await expect(review(page)).toContainText("Poznámka od AI");
    await undo(page);
    await waitForProject(id, (p) => !p.notes);
    await expect(review(page)).toHaveCount(0);
    const history = await mcp.call("get_history", { id });
    expect(history[0].actor).toBe("user");
    expect(history[0].kind).toBe("revert");
    expect(history[1].undone).toBe(true);
    // Redo takes the revert back: the AI's change waits again.
    await redo(page);
    await waitForProject(id, (p) => p.notes === "Od AI");
    await expect(review(page)).toContainText("Poznámka od AI");
    expect(await errorToasts(page).count()).toBe(0);
  });

  test("the AI's own revert of its only step leaves nothing to review (ERR-1601)", async ({ app, page, mcp }) => {
    const name = "R17 AI zpět";
    const id = await createExample("gable", name);
    await openEditor(app, name);
    await mcp.call("update_job", { id, notes: "Od AI", summary: "Poznámka od AI" });
    await expect(review(page)).toContainText("Poznámka od AI");
    await mcp.call("revert_last_change", { id, summary: "Omyl" });
    await waitForProject(id, (p) => !p.notes);
    await expect(review(page)).toHaveCount(0);
    const status = await mcp.call("get_review", { id });
    expect(status.pending).toBeNull();
  });

  test("a job the AI created: Revert all keeps the creation, and the lone creation offers Delete job (HUNT-1601)", async ({ app, page, mcp }) => {
    const created = await mcp.call("create_from_example", { example_id: "gable", name: "R17 od AI", summary: "Založeno podle výkresu" });
    const id = created.id as string;
    await mcp.call("update_job", { id, notes: "Od AI", summary: "Poznámka od AI" });
    await mcp.call("update_job", { id, customer: "AI Zákazník", summary: "Zákazník od AI" });
    await app.open({ prefs: { followAi: false } });
    await app.openJob("R17 od AI");
    await expect(review(page)).toContainText(cs("review.steps", { n: 3 }));
    await app.answerConfirm(true);
    await review(page).getByRole("button", { name: cs("review.revertAll") }).click();
    await waitForProject(id, (p) => !p.notes && p.job.customer === "");
    await noToast(errorToasts(page), 1500, "an error toast (revert_created)");
    // Only the creation is left: Accept or Delete job, no Revert.
    await expect(review(page)).toContainText("Založeno podle výkresu");
    await expect(review(page).getByRole("button", { name: cs("ai.revert"), exact: true })).toHaveCount(0);
    await dockTab(page, "dock.ai").click();
    await expect(page.locator("aside .panel").getByRole("button", { name: cs("ai.revert"), exact: true })).toHaveCount(0);
    await app.answerConfirm(true);
    await review(page).getByTestId("review-delete").click();
    await expect(page.locator("header .brand")).toBeVisible();
    await expect(app.toasts().filter({ hasText: cs("home.deleted", { name: "R17 od AI" }) })).toHaveCount(1);
    await expect(page.getByRole("row").filter({ hasText: "R17 od AI" })).toHaveCount(0);
  });
});
