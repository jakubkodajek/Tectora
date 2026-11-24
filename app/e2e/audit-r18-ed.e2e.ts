// Guards for the round-18 audit (2026-11-24), editor side.
//
// Design B: every Undo / Redo through the core names the newest entry the
// editor has applied (seenTs); the core refuses with revert_stale only when
// the log is newer, and presses made meanwhile are queued.
// TEST-1701 t18a  Undo, Redo, Undo again over Revert all: the third press goes through the core
// TEST-1702 t18c  Revert all over an AI's revert of accepted work starts where the core says
// TEST-1703 t18b  Revert all that finds nothing to revert says so
// ARCH-1703 / HUNT-1703  Undo, Undo, Redo, Redo over two AI saves
// known-open      two review Reverts, then Ctrl+Z twice
// ERR-1702        an AI save while Revert is out is undone by a logged revert
// ERR-1704        an AI save during the Undo's call is not reverted unseen
// UI-1704         a second Ctrl+Z during the first one's call is not dropped
// Design E (editor): UX-1703 / UI-1702 Restore in the Delete job notice, a step
// during the question is not deleted; HUNT-1706 a failed delete reads the job.
// Design F: ERR-1705 the question counts no creation; UX-1702 / TEST-1706 the
// newer-format exits; UI-1701 Retry fits at 960x540; HUNT-1707 the folder
// request names the AI client and wraps a long reason.
import fs from "node:fs";
import path from "node:path";
import type { Locator, Page } from "@playwright/test";
import { bar, openEditor, selectWall, setField } from "./editor";
import { E2E_DIR, LIBRARY_DIR } from "./env";
import { bridge, createExample, expect, listProjects, noToast, test, waitForProject } from "./fixtures";
import { cs, tr } from "./i18n";

// TECTORA_SHOTS=<folder>/ saves a screenshot after each test (fix evidence).
test.afterEach(async ({ page }, info) => {
  const dir = process.env.TECTORA_SHOTS;
  if (!dir) return;
  const id = info.title.match(/(HUNT|UX|UI|ERR|TEST|ARCH)-\d+/)?.[0] ?? "r18";
  const n = info.title.replace(/[^a-z0-9]+/gi, "-").slice(0, 40);
  await page.screenshot({ path: `${dir}r18-${id}-${n}-${process.env.TECTORA_SHOTS_TAG ?? "after"}.png`, timeout: 10_000 }).catch(() => {});
});

const review = (page: Page) => page.locator(".review");
const dockTab = (page: Page, key: string) => page.locator("aside .tabs").getByRole("tab", { name: cs(key) });
const entries = (page: Page) => page.getByTestId("history-entry");
const errorToasts = (page: Page) => page.locator(".stack .toast.error");
const undo = (page: Page) => page.locator("body").press("ControlOrMeta+z");
const redo = (page: Page) => page.locator("body").press("ControlOrMeta+Shift+z");
const toastsOf = (app: { toasts(): Locator }, text: string) => app.toasts().filter({ hasText: text });
const footerSave = (page: Page) => page.locator(".statusbar .save");
/** The Job tab's customer field: shows the job the editor has applied (the banner may be read before it). */
async function customerShown(page: Page, value: string) {
  // The reload that applies the job may switch the dock to the AI tab: look again until it shows.
  await expect(async () => {
    await dockTab(page, "dock.job").click();
    await expect(page.getByRole("tabpanel").getByLabel(cs("job.customer"), { exact: true })).toHaveValue(value, { timeout: 500 });
  }).toPass({ timeout: 8000 });
}

/** The next confirm() waits for the test: `answer(true|false)` gives the user's answer. */
async function holdConfirm(page: Page) {
  await page.evaluate(() => {
    const w = window as any;
    w.confirm = (m: string) => {
      w.__e2e.dialogs.push({ kind: "confirm", message: String(m) });
      return new Promise((res) => (w.__answer = res));
    };
  });
  await page.evaluate(() => delete (window as any).__answer);
  return {
    asked: () => expect.poll(() => page.evaluate(() => typeof (window as any).__answer)).toBe("function"),
    answer: (yes: boolean) => page.evaluate((y) => (window as any).__answer(y), yes),
  };
}

/** Two AI saves of the open job, each read by the editor as its own outside save. */
async function twoAiSaves(page: Page, mcp: any, id: string) {
  await mcp.call("update_job", { id, notes: "Od AI", summary: "Poznámka od AI" });
  await expect(review(page)).toContainText("Poznámka od AI");
  await mcp.call("update_job", { id, customer: "AI Zákazník", summary: "Zákazník od AI" });
  await expect(review(page)).toContainText(cs("review.steps", { n: 2 }));
}

test.describe("round 18, design B: Undo and Redo name what the editor has seen", () => {
  test("t18a Undo, Redo, Undo again over Revert all: the third press goes through the core too (TEST-1701)", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "T18 třikrát");
    await openEditor(app, "T18 třikrát");
    await twoAiSaves(page, mcp, id);
    await app.answerConfirm(true);
    await review(page).getByRole("button", { name: cs("review.revertAll") }).click();
    await waitForProject(id, (p) => !p.notes && p.job.customer === "");
    // Keys are refused until Revert all's answer is in (round 12 design A1): wait for it.
    await expect(review(page)).toHaveCount(0);
    await expect(footerSave(page)).toHaveText(cs("editor.saved"));
    await undo(page);
    await waitForProject(id, (p) => p.notes === "Od AI");
    await expect(review(page)).toContainText(cs("review.steps", { n: 2 }));
    await redo(page);
    await waitForProject(id, (p) => !p.notes && p.job.customer === "");
    await expect(review(page)).toHaveCount(0);
    const before = (await mcp.call("get_history", { id })).length;
    await undo(page);
    await waitForProject(id, (p) => p.notes === "Od AI" && p.job.customer === "AI Zákazník");
    await expect(review(page)).toContainText(cs("review.steps", { n: 2 }));
    const h = await mcp.call("get_history", { id });
    expect(h.length).toBe(before + 1);
    expect([h[0].kind, h[0].actor]).toEqual(["revert", "user"]);
    expect((await mcp.call("get_review", { id })).pending?.count).toBe(2);
    expect(await errorToasts(page).count()).toBe(0);
  });

  test("t18b Revert all that finds nothing to revert says so and the banner goes (TEST-1703)", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "T18 nic");
    await openEditor(app, "T18 nic");
    await twoAiSaves(page, mcp, id);
    const q = await holdConfirm(page);
    await review(page).getByRole("button", { name: cs("review.revertAll") }).click();
    await q.asked();
    // The AI reverts both of its steps while the question is open.
    const h = await mcp.call("get_history", { id });
    const a1 = h.find((e: any) => e.summary === "Poznámka od AI").ts_ms;
    await mcp.call("revert_to", { id, ts_ms: a1, summary: "Zpět vše" });
    await waitForProject(id, (p) => !p.notes && p.job.customer === "");
    await q.answer(true);
    await expect(toastsOf(app, cs("review.nothingToRevert"))).toHaveCount(1);
    await expect(review(page)).toHaveCount(0);
  });

  test("t18c Revert all over an AI's revert of accepted work starts where the core says (TEST-1702)", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "T18 přijato");
    await mcp.call("update_job", { id, notes: "Od AI", summary: "Poznámka od AI" });
    const log = await bridge<{ ts_ms: number; summary: string }[]>("project_activity", { id });
    const a1 = log.find((e) => e.summary === "Poznámka od AI")!.ts_ms;
    await bridge("accept_changes", { id, tsMs: a1 });
    await mcp.call("update_job", { id, customer: "AI Zákazník", summary: "Zákazník od AI" });
    await mcp.call("revert_to", { id, ts_ms: a1, summary: "Zpět obojí" });
    await waitForProject(id, (p) => !p.notes && p.job.customer === "");
    await app.open({ prefs: { followAi: false } });
    await app.openJob("T18 přijato");
    await expect(review(page)).toBeVisible();
    await app.answerConfirm(true);
    await review(page).getByRole("button", { name: cs("ai.revert"), exact: true }).or(review(page).getByRole("button", { name: cs("review.revertAll") })).first().click();
    const p = await waitForProject(id, (p) => p.notes === "Od AI");
    expect(p.job.customer).toBe("");
    await page.waitForTimeout(500);
    expect((await waitForProject(id, () => true)).job.customer).toBe("");
    await expect(review(page)).toHaveCount(0);
  });

  test("Undo, Undo, Redo, Redo over two AI saves ends with both in the job, waiting, and no error (ARCH-1703)", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "R18 tam a zpět");
    await openEditor(app, "R18 tam a zpět");
    await twoAiSaves(page, mcp, id);
    await undo(page);
    await waitForProject(id, (p) => p.notes === "Od AI" && p.job.customer === "");
    await undo(page);
    await waitForProject(id, (p) => !p.notes && p.job.customer === "");
    await expect(review(page)).toHaveCount(0);
    await redo(page);
    await waitForProject(id, (p) => p.notes === "Od AI" && p.job.customer === "");
    await redo(page);
    await waitForProject(id, (p) => p.notes === "Od AI" && p.job.customer === "AI Zákazník");
    await expect(review(page)).toContainText(cs("review.steps", { n: 2 }));
    expect((await mcp.call("get_review", { id })).pending?.count).toBe(2);
    await noToast(errorToasts(page), 1000, "an error toast (revert_stale)");
    await expect(toastsOf(app, cs("err.revert_stale"))).toHaveCount(0);
  });

  test("two review Reverts, then Ctrl+Z twice brings both AI steps back (known-open round 17)", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "R18 dva reverty");
    await openEditor(app, "R18 dva reverty");
    await twoAiSaves(page, mcp, id);
    // The AI tab's Revert of the newest step, then the banner's Revert of the other.
    await dockTab(page, "dock.ai").click();
    await entries(page).first().getByRole("button", { name: cs("ai.revert"), exact: true }).click();
    await waitForProject(id, (p) => p.notes === "Od AI" && p.job.customer === "");
    await expect(review(page)).not.toContainText(cs("review.steps", { n: 2 }));
    await expect(footerSave(page)).toHaveText(cs("editor.saved"));
    await review(page).getByRole("button", { name: cs("ai.revert"), exact: true }).click();
    await waitForProject(id, (p) => !p.notes && p.job.customer === "");
    await expect(review(page)).toHaveCount(0);
    await expect(footerSave(page)).toHaveText(cs("editor.saved"));
    await undo(page);
    await waitForProject(id, (p) => p.notes === "Od AI" && p.job.customer === "");
    await undo(page);
    await waitForProject(id, (p) => p.notes === "Od AI" && p.job.customer === "AI Zákazník");
    await expect(review(page)).toContainText(cs("review.steps", { n: 2 }));
    await noToast(errorToasts(page), 1000, "an error toast (revert_stale)");
  });

  test("an AI save while Revert is out comes in as a step whose Ctrl+Z is a logged revert (ERR-1702)", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "R18 během revertu");
    await openEditor(app, "R18 během revertu");
    await mcp.call("update_job", { id, notes: "Od AI", summary: "Poznámka od AI" });
    await expect(review(page)).toContainText("Poznámka od AI");
    // The core has reverted; its answer is held while the AI saves again.
    let open!: () => void;
    const gate = new Promise<void>((r) => (open = r));
    let n = 0;
    await page.route("**/revert_changes", async (route) => {
      const res = await route.fetch();
      n++;
      await gate;
      await route.fulfill({ response: res }).catch(() => {});
    });
    await review(page).getByRole("button", { name: cs("ai.revert"), exact: true }).click();
    await expect.poll(() => n).toBe(1);
    await mcp.call("update_job", { id, customer: "AI Zákazník", summary: "Zákazník od AI" });
    await waitForProject(id, (p) => !p.notes && p.job.customer === "AI Zákazník");
    // Read while the answer is held: the review shows it already.
    await expect.poll(async () => (await mcp.call("get_review", { id })).pending?.count).toBe(1);
    open();
    await expect(review(page)).toContainText("Zákazník od AI");
    await customerShown(page, "AI Zákazník");
    await undo(page);
    await waitForProject(id, (p) => p.job.customer === "");
    const h = await mcp.call("get_history", { id });
    expect([h[0].kind, h[0].actor]).toEqual(["revert", "user"]);
    expect((await mcp.call("get_review", { id })).pending).toBeNull();
    await expect(review(page)).toHaveCount(0);
  });

  test("an AI save during the Undo's call is not reverted unseen: nothing is undone, a note, the save arrives as its own step (ERR-1704)", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "R18 během zpět");
    await openEditor(app, "R18 během zpět");
    await mcp.call("update_job", { id, notes: "Od AI", summary: "Poznámka od AI" });
    await expect(review(page)).toContainText("Poznámka od AI");
    // The AI's next save lands just before the Undo's call reaches the core.
    let first = true;
    await page.route("**/revert_changes", async (route) => {
      if (first) {
        first = false;
        await mcp.call("update_job", { id, customer: "AI Zákazník", summary: "Zákazník od AI" });
      }
      await route.continue();
    });
    await undo(page);
    await expect(toastsOf(app, cs("err.action.undo"))).toHaveCount(1);
    await expect(toastsOf(app, cs("err.action.undo"))).toContainText(cs("err.revert_stale"));
    app.errors.length = 0;
    let p = await waitForProject(id, (p) => p.job.customer === "AI Zákazník");
    expect(p.notes).toBe("Od AI");
    await expect(review(page)).toContainText(cs("review.steps", { n: 2 }));
    await customerShown(page, "AI Zákazník");
    expect((await mcp.call("get_review", { id })).pending?.count).toBe(2);
    // The next Ctrl+Z undoes the AI's newest save only.
    await undo(page);
    p = await waitForProject(id, (p) => p.job.customer === "");
    expect(p.notes).toBe("Od AI");
    await expect(review(page)).toContainText("Poznámka od AI");
    await expect(review(page)).not.toContainText(cs("review.steps", { n: 2 }));
  });

  test("a second Ctrl+Z while the first one's call is out is not dropped: both go through (UI-1704)", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "R18 rychle zpět");
    await openEditor(app, "R18 rychle zpět");
    await twoAiSaves(page, mcp, id);
    // The first press's call is out (its answer a little slow) when the second comes.
    let calls = 0;
    await page.route("**/revert_changes", async (route) => {
      calls++;
      const res = await route.fetch();
      if (calls === 1) await new Promise((r) => setTimeout(r, 400));
      await route.fulfill({ response: res }).catch(() => {});
    });
    await undo(page);
    await expect.poll(() => calls).toBe(1);
    await page.waitForTimeout(40);
    await undo(page);
    await waitForProject(id, (p) => !p.notes && p.job.customer === "");
    await expect(review(page)).toHaveCount(0);
    const h = await mcp.call("get_history", { id });
    expect(h.slice(0, 2).map((e: any) => [e.kind, e.actor])).toEqual([
      ["revert", "user"],
      ["revert", "user"],
    ]);
    expect(await errorToasts(page).count()).toBe(0);
  });
});

test.describe("round 18, design E (editor): Delete job", () => {
  test("Delete job offers Restore at once, and Restore brings the job and its review back (UX-1703)", async ({ app, page, mcp }) => {
    const created = await mcp.call("create_from_example", { example_id: "gable", name: "R18 smazat", summary: "Založeno podle výkresu" });
    const id = created.id as string;
    await app.open({ prefs: { followAi: false } });
    await app.openJob("R18 smazat");
    await expect(review(page).getByTestId("review-delete")).toBeVisible();
    await app.answerConfirm(true);
    await review(page).getByTestId("review-delete").focus();
    await page.keyboard.press("Enter");
    await expect(page.locator("header .brand")).toBeVisible();
    const toast = toastsOf(app, cs("home.deleted", { name: "R18 smazat" }));
    await expect(toast).toHaveCount(1);
    // The focus is in the library, not on the page (UI-1702).
    await expect.poll(() => page.evaluate(() => document.activeElement?.tagName ?? "")).not.toBe("BODY");
    await toast.getByRole("button", { name: cs("home.restore") }).click();
    await expect(toastsOf(app, cs("home.restored", { name: "R18 smazat" }))).toHaveCount(1);
    await expect(page.getByRole("row").filter({ hasText: "R18 smazat" })).toHaveCount(1);
    expect((await listProjects()).some((p) => p.id === id)).toBe(true);
    await app.openJob("R18 smazat");
    await expect(review(page)).toContainText("Založeno podle výkresu");
    await expect(review(page).getByTestId("review-delete")).toBeVisible();
  });

  test("an AI step that comes in while Delete job is asked is not deleted with the job (UI-1702)", async ({ app, page, mcp }) => {
    const created = await mcp.call("create_from_example", { example_id: "gable", name: "R18 otázka", summary: "Založeno podle výkresu" });
    const id = created.id as string;
    await app.open({ prefs: { followAi: false } });
    await app.openJob("R18 otázka");
    const q = await holdConfirm(page);
    await review(page).getByTestId("review-delete").click();
    await q.asked();
    await mcp.call("update_job", { id, notes: "Od AI", summary: "Poznámka od AI" });
    await waitForProject(id, (p) => p.notes === "Od AI");
    await q.answer(true);
    await expect(toastsOf(app, cs("review.deleteChanged"))).toHaveCount(1);
    await expect(page.locator(".editor")).toBeVisible();
    expect((await listProjects()).some((p) => p.id === id)).toBe(true);
    await expect(review(page)).toContainText(cs("review.steps", { n: 2 }));
  });

  test("a Delete job that fails reads the job again: an AI save made meanwhile shows (HUNT-1706)", async ({ app, page, mcp }) => {
    const created = await mcp.call("create_from_example", { example_id: "gable", name: "R18 nesmazáno", summary: "Založeno podle výkresu" });
    const id = created.id as string;
    await app.open({ prefs: { followAi: false } });
    await app.openJob("R18 nesmazáno");
    await page.route("**/delete_project", async (route) => {
      await mcp.call("update_job", { id, customer: "AI Zákazník", summary: "Zákazník od AI" });
      // Let the file event of the AI's save reach the editor while the delete is out.
      await new Promise((r) => setTimeout(r, 800));
      await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ err: { code: "io", id, detail: "the trash is not writable" } }) }).catch(() => {});
    });
    await app.answerConfirm(true);
    await review(page).getByTestId("review-delete").click();
    await expect(toastsOf(app, cs("err.action.delete"))).toHaveCount(1);
    app.errors.length = 0;
    await expect(review(page)).toContainText(cs("review.steps", { n: 2 }));
    await expect(review(page)).toContainText("Zákazník od AI");
    await expect(review(page).getByTestId("review-delete")).toHaveCount(0);
  });
});

test.describe("round 18, design F", () => {
  test("Revert all on a job the AI created counts and promises only the steps after the creation (ERR-1705)", async ({ app, page, mcp }) => {
    const created = await mcp.call("create_from_example", { example_id: "gable", name: "R18 od AI", summary: "Založeno podle výkresu" });
    const id = created.id as string;
    await mcp.call("update_job", { id, notes: "Od AI", summary: "Poznámka od AI" });
    await mcp.call("update_job", { id, customer: "AI Zákazník", summary: "Zákazník od AI" });
    await app.open({ prefs: { followAi: false } });
    await app.openJob("R18 od AI");
    await expect(review(page)).toContainText(cs("review.steps", { n: 3 }));
    await app.answerConfirm(true);
    await review(page).getByRole("button", { name: cs("review.revertAll") }).click();
    await waitForProject(id, (p) => !p.notes && p.job.customer === "");
    const asked = (await app.dialogs()).filter((d) => d.kind === "confirm").map((d) => d.message);
    expect(asked).toEqual([cs("review.confirmRevertAllCreated", { n: 2 })]);
    // Creation + one step: one step to revert, the button says Revert, no question.
    await mcp.call("update_job", { id, notes: "Znovu", summary: "Poznámka znovu" });
    await expect(review(page)).toContainText("Poznámka znovu");
    await expect(review(page).getByRole("button", { name: cs("review.revertAll") })).toHaveCount(0);
    await review(page).getByRole("button", { name: cs("ai.revert"), exact: true }).click();
    await waitForProject(id, (p) => !p.notes);
    expect((await app.dialogs()).filter((d) => d.kind === "confirm")).toHaveLength(1);
  });

  test("a newer Tectora's file replaced by a good one: the edit made meanwhile is saved, the stale advice goes (UX-1702)", async ({ app, page }) => {
    const name = "R18 novější zpět";
    const id = await createExample("hip", name);
    await openEditor(app, name);
    await app.saved();
    const file = path.join(LIBRARY_DIR, `${id}.tectora`);
    const good = fs.readFileSync(file, "utf8");
    const newer = JSON.parse(good);
    newer.schema_version = 99;
    fs.writeFileSync(file, JSON.stringify(newer));
    await expect(toastsOf(app, cs("editor.outsideNewer"))).toHaveCount(1);
    await selectWall(page, 0);
    await setField(bar(page, "def.title").getByLabel(cs("edge.pitch")), 41);
    const failed = toastsOf(app, cs("err.ofJob", { name, title: cs("err.action.save") }));
    await expect(failed).toHaveCount(1);
    app.errors.length = 0;
    await expect(footerSave(page)).toContainText(cs("editor.saveFailed"));
    // The other program writes the good file back.
    fs.writeFileSync(file, good);
    const p = await waitForProject(id, (q) => q.parts[0].edges[0].pitch_deg === 41);
    expect(p.schema_version).not.toBe(99);
    await expect(failed).toHaveCount(0);
    await expect(toastsOf(app, cs("editor.outsideNewer"))).toHaveCount(0);
    await expect(footerSave(page)).toHaveText(cs("editor.saved"));
  });

  test("a newer Tectora's file, then one 40 levels deep: the unreadable notice replaces the newer one (TEST-1706)", async ({ app, page }) => {
    const name = "R18 novější hloubka";
    const id = await createExample("hip", name);
    await openEditor(app, name);
    await app.saved();
    const file = path.join(LIBRARY_DIR, `${id}.tectora`);
    const good = fs.readFileSync(file, "utf8");
    const newer = JSON.parse(good);
    newer.schema_version = 99;
    fs.writeFileSync(file, JSON.stringify(newer));
    await expect(toastsOf(app, cs("editor.outsideNewer"))).toHaveCount(1);
    const deep = JSON.parse(good);
    let v: unknown = {};
    for (let k = 1; k < 40; k++) v = { a: v };
    deep.crm = v;
    deep.notes = "from CRM";
    fs.writeFileSync(file, JSON.stringify(deep));
    await expect(toastsOf(app, cs("editor.outsideUnreadable"))).toHaveCount(1);
    await expect(toastsOf(app, cs("editor.outsideNewer"))).toHaveCount(0);
    await expect(footerSave(page)).toHaveText(cs("editor.outsideState"));
  });

  test("Retry of a failed save fits the German status bar at 960x540 (UI-1701)", async ({ app, page }) => {
    await page.setViewportSize({ width: 960, height: 540 });
    const name = "R18 schmal";
    const id = await createExample("hip", name);
    await app.open({ locale: "de" });
    await app.openJob(name);
    await app.saved();
    const file = path.join(LIBRARY_DIR, `${id}.tectora`);
    const newer = JSON.parse(fs.readFileSync(file, "utf8"));
    newer.schema_version = 99;
    fs.writeFileSync(file, JSON.stringify(newer));
    await expect(footerSave(page)).toHaveText(tr("de", "editor.outsideNewerState"));
    await page.locator(".view .wrap > svg g.edge line.hit").first().dispatchEvent("pointerdown", { button: 0 });
    const defBar = page.getByRole("toolbar", { name: tr("de", "def.title") });
    await expect(defBar.locator(".which strong")).toHaveText(tr("de", "def.wall", { n: 1 }));
    await setField(defBar.getByLabel(tr("de", "edge.pitch")), 41);
    const retry = page.locator(".statusbar .retry");
    await expect(retry).toHaveText(tr("de", "err.retry"));
    app.errors.length = 0;
    const box = await retry.boundingBox();
    expect(box).not.toBeNull();
    const bar_ = await page.locator("footer.statusbar").boundingBox();
    expect(box!.x).toBeGreaterThanOrEqual(0);
    expect(box!.x + box!.width).toBeLessThanOrEqual(Math.min(960, bar_!.x + bar_!.width) - 1);
    // The button's own text is not cut.
    expect(await retry.evaluate((b) => b.scrollWidth <= b.clientWidth)).toBe(true);
  });

  test("the folder request names the AI client as the app does and wraps an unbroken reason (HUNT-1707)", async ({ app, page, mcp }) => {
    await app.open();
    const other = path.join(E2E_DIR, "r18-folder");
    fs.rmSync(other, { recursive: true, force: true });
    fs.mkdirSync(other, { recursive: true });
    const why = "x".repeat(499);
    await mcp.call("update_settings", { projects_dir: other, summary: why });
    const dlg = page.getByRole("dialog", { name: cs("folderReq.title") });
    const reason = dlg.getByTestId("folder-reason");
    await expect(reason).toBeVisible();
    expect((await reason.innerText()).trim().startsWith(cs("folderReq.reason", { by: "Claude" }))).toBe(true);
    await expect(dlg).toContainText(cs("folderReq.lead", { by: "Claude" }));
    expect(await reason.evaluate((e) => e.scrollWidth <= e.clientWidth)).toBe(true);
    await dlg.getByRole("button", { name: cs("folderReq.keep") }).click();
    await expect(dlg).toHaveCount(0);
  });
});
