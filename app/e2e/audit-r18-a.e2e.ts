// Guards for the round-18 audit (2026-11-24), designs A, C and D: a revert
// is read by what it did.
//
// ARCH-1701 the AI's take-back of the user's revert of accepted work puts
//           the change back: the banner comes up (owner decision 1)
// ERR-1703  the history line of a Redo of Revert all is one short sentence,
//           never "Reverted: Reverted: …" (proposed t18d)
// UX-1701   with the AI's own revert in the chain, Undo reads "AI change
//           put back" and Redo "AI change reverted" (owner decision 3)
// ERR-1701  a Revert of a change the user already typed back ends the review
import type { Page } from "@playwright/test";
import { openEditor } from "./editor";
import { bridge, createExample, expect, test, waitForProject } from "./fixtures";
import { cs } from "./i18n";

// TECTORA_SHOTS=<folder>/ saves a screenshot after each test (fix evidence).
test.afterEach(async ({ page }, info) => {
  const dir = process.env.TECTORA_SHOTS;
  if (!dir) return;
  const id = info.title.match(/(HUNT|UX|UI|ERR|TEST|ARCH)-\d+/)?.[0] ?? "r18";
  await page.screenshot({ path: `${dir}r18-${id}-${process.env.TECTORA_SHOTS_TAG ?? "after"}.png`, timeout: 10_000 }).catch(() => {});
});

const review = (page: Page) => page.locator(".review");
const dockTab = (page: Page, key: string) => page.locator("aside .tabs").getByRole("tab", { name: cs(key) });
const entries = (page: Page) => page.getByTestId("history-entry");
const errorToasts = (page: Page) => page.locator(".stack .toast.error");
const undo = (page: Page) => page.locator("body").press("ControlOrMeta+z");
const redo = (page: Page) => page.locator("body").press("ControlOrMeta+Shift+z");

type Logged = { ts_ms: number; actor: string; summary: string };
const newest = async (id: string) => (await bridge<Logged[]>("project_activity", { id }))[0];

test.describe("round 18: a revert is read by what it did", () => {
  test("the AI putting back an accepted change the user reverted brings the banner up (ARCH-1701)", async ({ app, page, mcp }) => {
    const name = "R18 vráceno";
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
    expect((await mcp.call("get_review", { id })).pending).toBeNull();

    await mcp.call("revert_to", { id, ts_ms: r.ts_ms, summary: "Vracím poznámku." });
    await waitForProject(id, (p) => p.notes === "Od AI");
    const status = await mcp.call("get_review", { id });
    expect(status.pending?.count).toBe(1);
    await expect(review(page)).toBeVisible();
    // Revert all takes the note out again and ends the review.
    await app.answerConfirm(true);
    await review(page).getByRole("button", { name: cs("ai.revert"), exact: true }).click();
    await waitForProject(id, (p) => !p.notes);
    await expect(review(page)).toHaveCount(0);
    expect((await mcp.call("get_review", { id })).pending).toBeNull();
    await expect(errorToasts(page)).toHaveCount(0);
  });

  test("the history line of a Redo of Revert all is one short sentence (ERR-1703)", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "R18 věta");
    await openEditor(app, "R18 věta");
    await mcp.call("update_job", { id, notes: "Od AI", summary: "Poznámka od AI" });
    await mcp.call("update_job", { id, customer: "AI Zákazník", summary: "Zákazník od AI" });
    await expect(review(page)).toContainText(cs("review.steps", { n: 2 }));
    await app.answerConfirm(true);
    await review(page).getByRole("button", { name: cs("review.revertAll") }).click();
    await waitForProject(id, (p) => !p.notes);
    await expect(review(page)).toHaveCount(0);
    await undo(page);
    await waitForProject(id, (p) => p.notes === "Od AI");
    await expect(review(page)).toContainText(cs("review.steps", { n: 2 }));
    await redo(page);
    await waitForProject(id, (p) => !p.notes);
    await dockTab(page, "dock.ai").click();
    const top = entries(page).first().locator(".summary");
    await expect(top).toHaveText(cs("ai.revertedEntry"));
    await expect(entries(page).nth(1).locator(".summary")).toHaveText(cs("ai.revertTakenBack"));
    // The stored line the library card and the AI read: one sentence.
    const last = await newest(id);
    expect(last.summary).toBe("Reverted 2 changes since: Poznámka od AI");
    await expect(errorToasts(page)).toHaveCount(0);
  });

  test("Undo of the AI's own revert reads put back, Redo reads reverted (UX-1701)", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "R18 sled");
    await openEditor(app, "R18 sled");
    await mcp.call("update_job", { id, notes: "Od AI", summary: "Poznámka od AI" });
    await mcp.call("update_job", { id, customer: "AI Zákazník", summary: "Zákazník od AI" });
    await expect(review(page)).toContainText(cs("review.steps", { n: 2 }));
    await mcp.call("revert_last_change", { id, summary: "AI vrací" });
    await waitForProject(id, (p) => p.job.customer === "");
    await expect(review(page)).toContainText("Poznámka od AI");
    await expect(review(page)).not.toContainText("Zákazník od AI");
    const quoted = cs("ai.quoted", { s: "Zákazník od AI" });

    await undo(page);
    await waitForProject(id, (p) => p.job.customer === "AI Zákazník");
    await expect(review(page)).toContainText(cs("review.steps", { n: 2 }));
    await dockTab(page, "dock.ai").click();
    await expect(entries(page).first().locator(".summary")).toHaveText(cs("ai.putBackOne", { s: quoted }));

    await redo(page);
    await waitForProject(id, (p) => p.job.customer === "");
    await expect(entries(page).first().locator(".summary")).toHaveText(cs("ai.revertedOne", { s: quoted }));
    expect((await mcp.call("get_review", { id })).pending?.count).toBe(1);
    await expect(errorToasts(page)).toHaveCount(0);
  });

  test("typing the AI's change back by hand ends the review (ERR-1701, ADR 0007)", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "R18 nic");
    await openEditor(app, "R18 nic");
    await mcp.call("update_job", { id, notes: "Od AI", summary: "Poznámka od AI" });
    await expect(review(page)).toContainText("Poznámka od AI");
    // The other way to put it back: the editor saves the job as before.
    const job = await bridge<any>("load_project", { id });
    await bridge("save_project_checked", { id, project: { ...job, notes: "" }, base: job });
    await waitForProject(id, (p) => !p.notes);
    // The job is as the user settled it again: the review ends by itself,
    // with no Revert to press (ADR 0007, owner decision 1 of round 20; was:
    // the banner's Revert logged a revert that ended it).
    await expect(review(page)).toHaveCount(0);
    expect((await mcp.call("get_review", { id })).pending).toBeNull();
    await expect(errorToasts(page)).toHaveCount(0);
  });
});
