// Guards for the round-16 audit (2026-11-23), part A: AI review and revert.
//
// UX-1501   the user's Revert of the latest of several AI steps keeps the
//           review of the others (owner decision 1 of round 16)
// UX-1505 = UI-1501
//           Revert is drawn on the entry the core undoes: never on an AI's
//           own revert entry, and on the change before it
// TEST-1502 Revert all counts an AI's own revert that came in while it
//           asked as an AI step reverted too (undone_ai, not undone)
// ARCH-1501 the AI's words inside a revert sentence are quoted
import type { Page } from "@playwright/test";
import { openEditor } from "./editor";
import { createExample, expect, noToast, test, waitForProject } from "./fixtures";
import { cs } from "./i18n";

// TECTORA_SHOTS=<folder>/ saves a screenshot after each test (fix evidence).
test.afterEach(async ({ page }, info) => {
  const dir = process.env.TECTORA_SHOTS;
  if (!dir) return;
  const id = info.title.match(/(HUNT|UX|UI|ERR|TEST|ARCH)-\d+/)?.[0] ?? "r16";
  const n = info.title.replace(/[^a-z0-9]+/gi, "-").slice(0, 40);
  await page.screenshot({ path: `${dir}r16-${id}-${n}-${process.env.TECTORA_SHOTS_TAG ?? "after"}.png`, timeout: 10_000 }).catch(() => {});
});

const review = (page: Page) => page.locator(".review");
const dockTab = (page: Page, key: string) => page.locator("aside .tabs").getByRole("tab", { name: cs(key) });
const entries = (page: Page) => page.getByTestId("history-entry");
const revertIn = (page: Page, i: number) => entries(page).nth(i).getByRole("button", { name: cs("ai.revert"), exact: true });
const errorToasts = (page: Page) => page.locator(".stack .toast.error");

test.describe("round 16, part A: a Revert undoes the entry it is drawn on", () => {
  test("the AI tab's Revert of the newest of three AI steps keeps the review of the other two (UX-1501)", async ({ app, page, mcp }) => {
    const name = "R16 tři kroky";
    const id = await createExample("gable", name);
    await openEditor(app, name);
    await mcp.call("update_job", { id, notes: "Od AI", summary: "Poznámka od AI" });
    await expect(review(page)).toContainText("Poznámka od AI");
    await mcp.call("update_job", { id, customer: "AI Zákazník", summary: "Zákazník od AI" });
    await mcp.call("update_job", { id, address: "AI Adresa", summary: "Adresa od AI" });
    await expect(review(page)).toContainText(cs("review.steps", { n: 3 }));
    await dockTab(page, "dock.ai").click();
    await revertIn(page, 0).click();
    await waitForProject(id, (p) => p.job.address === "" && p.job.customer === "AI Zákazník");
    // The other two still wait, with Accept and Revert all.
    await expect(review(page)).toContainText(cs("review.steps", { n: 2 }));
    await expect(review(page).locator(".review-summary")).toHaveText("„Poznámka od AI“ → „Zákazník od AI“");
    await expect(app.toasts().filter({ hasText: cs("ai.revertedOne", { s: "„Adresa od AI“" }) })).toHaveCount(1);
    // The undone step is marked in the history, and the button moved to the next one.
    await expect(entries(page).nth(1).getByTestId("history-undone")).toHaveText(cs("ai.undone"));
    await expect(revertIn(page, 0)).toHaveCount(0);
    await expect(revertIn(page, 2)).toHaveCount(1);
    await app.answerConfirm(true);
    await review(page).getByRole("button", { name: cs("review.revertAll") }).click();
    await waitForProject(id, (p) => !p.notes && p.job.customer === "" && p.job.address === "");
    await expect(review(page)).toHaveCount(0);
    expect(await errorToasts(page).count()).toBe(0);
  });

  test("Accept still works for the steps left after a one-step Revert (UX-1501)", async ({ app, page, mcp }) => {
    const name = "R16 přijmout";
    const id = await createExample("gable", name);
    await openEditor(app, name);
    await mcp.call("update_job", { id, notes: "Od AI", summary: "Poznámka od AI" });
    await mcp.call("update_job", { id, customer: "AI Zákazník", summary: "Zákazník od AI" });
    await mcp.call("update_job", { id, address: "AI Adresa", summary: "Adresa od AI" });
    await expect(review(page)).toContainText(cs("review.steps", { n: 3 }));
    await dockTab(page, "dock.ai").click();
    await revertIn(page, 0).click();
    await expect(review(page)).toContainText(cs("review.steps", { n: 2 }));
    await review(page).getByRole("button", { name: cs("review.accept") }).click();
    await expect(review(page)).toHaveCount(0);
    const status = await mcp.call("get_review", { id });
    expect(status.pending).toBeNull();
    const p = await waitForProject(id, (p) => p.notes === "Od AI");
    expect([p.job.customer, p.job.address]).toEqual(["AI Zákazník", ""]);
  });

  test("an AI's own revert of its only change gets no Revert button (UI-1501)", async ({ app, page, mcp }) => {
    const name = "R16 AI vrátila";
    const id = await createExample("gable", name);
    await openEditor(app, name);
    await mcp.call("update_job", { id, notes: "Od AI", summary: "Poznámka od AI" });
    await expect(review(page)).toContainText("Poznámka od AI");
    await mcp.call("revert_last_change", { id, summary: "Omyl" });
    await waitForProject(id, (p) => !p.notes);
    await dockTab(page, "dock.ai").click();
    await expect(entries(page)).toHaveCount(2);
    await expect(page.locator("aside .panel").getByRole("button", { name: cs("ai.revert"), exact: true })).toHaveCount(0);
    // The AI's words inside Tectora's sentence are quoted (ARCH-1501).
    await expect(entries(page).first().locator(".summary")).toContainText("„Poznámka od AI“");
    await expect(entries(page).nth(1).getByTestId("history-undone")).toHaveCount(1);
  });

  test("after two AI changes and the AI's revert of the second, Revert is on the first and undoes it (UX-1505, TEST-1503)", async ({ app, page, mcp }) => {
    const name = "R16 starší";
    const id = await createExample("gable", name);
    await openEditor(app, name);
    await mcp.call("update_job", { id, notes: "Od AI", summary: "Poznámka od AI" });
    await expect(review(page)).toContainText("Poznámka od AI");
    await mcp.call("update_job", { id, customer: "AI Zákazník", summary: "Zákazník od AI" });
    await mcp.call("revert_last_change", { id, summary: "Zpět zákazník" });
    await waitForProject(id, (p) => p.job.customer === "" && p.notes === "Od AI");
    await dockTab(page, "dock.ai").click();
    // Newest first: the AI's revert, the reverted change, the first change.
    await expect(entries(page)).toHaveCount(3);
    await expect(revertIn(page, 0)).toHaveCount(0);
    await expect(revertIn(page, 1)).toHaveCount(0);
    await expect(entries(page).nth(2).locator(".summary")).toHaveText("„Poznámka od AI“");
    await revertIn(page, 2).click();
    await waitForProject(id, (p) => !p.notes && p.job.customer === "");
    await expect(app.toasts().filter({ hasText: cs("ai.revertedOne", { s: "„Poznámka od AI“" }) })).toHaveCount(1);
    await noToast(errorToasts(page), 1500, "an error toast");
  });

  test("Revert all counts an AI's own revert that came in while it asked as an AI step reverted too (TEST-1502)", async ({ app, page, mcp }) => {
    const name = "R16 počet";
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
    // The AI reverts its own latest step meanwhile: A1 A2 Ar, the banner showed 2,
    // the AI's entries undone are 3 (undone counts 2 changes).
    await mcp.call("revert_last_change", { id, summary: "Zpět zákazník" });
    await page.evaluate(() => (window as any).__answer(true));
    await waitForProject(id, (p) => !p.notes && p.job.customer === "");
    await expect(app.toasts().filter({ hasText: cs("review.revertedMore", { n: 1 }) })).toHaveCount(1);
  });
});
