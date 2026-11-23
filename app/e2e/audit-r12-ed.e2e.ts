// Guards for the round-12 audit (2026-11-23), the editor's half: fix design 1
// "one writer gate" (HUNT-1101, ERR-1101, UI-1101, HUNT-1103, ERR-1102,
// ERR-1105, TEST-1101, TEST-1105) and the banner / wording fixes (ERR-1103,
// UI-1103, UX-1101, UX-1102).
//
// The preview's bridge answers one request at a time (TEST-1102): a command
// sent while Revert waits for the job's lock would be answered only after
// it, the one order in which the bugs cannot show. So the call is held at
// the browser (`hold` of review.ts: the core ran it, its answer waits), and
// every other command is answered at once, as under Tauri.
//
// The intruders, each tried while the call is out:
//   draw-enter            Enter in the footprint bar adds no building the answer would throw away (ERR-1101)
//   async-answer-status   a status change answered by the core during the press lands before the call (HUNT-1101)
//   async-answer-store    the card's Store answered during the press lands before the call (HUNT-1101)
//   toast-action          a toast's action does nothing and the toast stays (UI-1101)
//   settings-key          Ctrl/Cmd+, opens no settings (HUNT-1103)
//   leave-open            the follow offer's "Open" does not leave; a busy answer names the job (ERR-1105)
//   ai-save               an AI save during the held answer is on screen after it, for all three calls (TEST-1101)
//   presave-held-lock     with a save waiting for another writer's lock the press answers busy within ~3 s and the editor stays usable (ERR-1102)
import type { Page } from "@playwright/test";
import { bar, clickPlan, selectWall } from "./editor";
import { FILES_DIR } from "./env";
import { bridge, createExample, expect, loadProject, test, waitForProject } from "./fixtures";
import { cs } from "./i18n";
import { acceptAll, hold, holdLock, olderOffer } from "./review";
import fs from "node:fs";
import path from "node:path";

// TECTORA_SHOTS=<folder>/ saves a screenshot after each test (fix evidence).
test.afterEach(async ({ page }, info) => {
  const dir = process.env.TECTORA_SHOTS;
  if (!dir) return;
  const id = info.title.match(/(HUNT|UX|UI|ERR|TEST)-\d+/)?.[0] ?? "r12";
  const n = info.title.replace(/[^a-z0-9]+/gi, "-").slice(0, 40);
  await page.screenshot({ path: `${dir}r12-${id}-${n}-${process.env.TECTORA_SHOTS_TAG ?? "after"}.png`, timeout: 10_000 }).catch(() => {});
});

const dockTab = (page: Page, key: string) => page.locator("aside .tabs").getByRole("tab", { name: cs(key) });
const entries = (page: Page) => page.getByTestId("history-entry");
const status = (page: Page) => page.locator(".statusbar .save");
const customer = (page: Page) => page.locator("aside .job").getByLabel(cs("job.customer"));
const statusSelect = (page: Page) => page.locator("aside .job select").first();
const historyRevert = (page: Page) => entries(page).first().getByRole("button", { name: cs("ai.revert") });
const bannerRevert = (page: Page) => page.locator(".review").getByRole("button", { name: cs("ai.revert") });
const bannerAccept = (page: Page) => page.locator(".review").getByRole("button", { name: cs("review.accept") });
const pitch = (page: Page) => bar(page, "def.title").getByLabel(cs("edge.pitch"));
const buildings = async (page: Page) => Number(((await page.locator(".statusbar").textContent()) ?? "").match(/budovy: (\d+)/)?.[1] ?? NaN);

/** The AI's change is in: the banner shows and the dock switched to the AI tab (it does so once the reload is in). */
async function aiShown(page: Page) {
  await expect(page.locator(".review")).toBeVisible();
  await expect(dockTab(page, "dock.ai")).toHaveAttribute("aria-selected", "true");
}

/** Keyboard focus on `l`, shown as such (`:focus-visible`), as a Tab user has it. */
async function keyboardFocus(page: Page, l: import("@playwright/test").Locator) {
  await l.focus();
  await page.keyboard.press("Shift+Tab");
  await page.keyboard.press("Tab");
  await expect(l).toBeFocused();
}

/** A gable job opened in the editor with one AI change (the notes) pending. */
async function readyNotes(app: any, page: Page, mcp: any, name: string, prefs: Record<string, unknown> = { followAi: false }) {
  const id = await createExample("gable", name);
  await app.open({ prefs });
  await app.openJob(name);
  await app.saved();
  await mcp.call("update_job", { id, notes: "Od AI", summary: "Poznámka od AI" });
  await waitForProject(id, (p) => p.notes === "Od AI");
  await app.saved();
  await aiShown(page);
  return id;
}

/** Answers `cmd` with the core's `busy` (its lock wait ran out) once released; the core is not asked. */
async function holdBusy(page: Page, cmd: string, id: string) {
  let open!: () => void;
  const gate = new Promise<void>((r) => (open = r));
  let n = 0;
  await page.route(`**/${cmd}`, async (route) => {
    n++;
    await gate;
    await route
      .fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ err: { code: "busy", id, detail: "the job is locked by another writer" } }) })
      .catch(() => {});
  });
  return { release: open, held: () => n };
}

/** The focus leaves the pressed button for the page, as after a press on the (inert) plan. */
const toBody = (page: Page) => page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());

test.describe("round 12: one writer gate while Revert or the put-back is out (fix design 1)", () => {
  test("draw-enter: Enter in the footprint bar adds no building while the call is out; the footprint stays to be closed after (ERR-1101)", async ({ app, page, mcp }) => {
    const id = await readyNotes(app, page, mcp, "R12 draw");
    await page.locator("body").press("Insert");
    const draw = page.getByRole("toolbar", { name: cs("draw.title") });
    await expect(draw).toBeVisible();
    await clickPlan(page, 13, 1);
    const len = draw.getByLabel(cs("draw.length"));
    await len.fill("4");
    await len.press("ArrowRight");
    await len.press("ArrowUp");
    await expect(draw).toContainText(cs("draw.points", { n: 3 }).split("·")[0].trim());
    await dockTab(page, "dock.ai").click();
    const held = await hold(page, "revert_last_change");
    await historyRevert(page).click();
    await expect.poll(() => held.held(), { timeout: 5000 }).toBe(1);
    await expect(status(page)).toHaveText(cs("editor.reverting"));
    await toBody(page);
    await page.keyboard.press("Enter");
    await page.keyboard.press("ControlOrMeta+i");
    await page.waitForTimeout(400);
    expect(await buildings(page), "no building added while the call is out").toBe(1);
    held.release();
    await waitForProject(id, (p) => !p.notes && p.parts.length === 1);
    await app.saved();
    await page.waitForTimeout(800);
    expect((await loadProject(id)).parts).toHaveLength(1);
    // The footprint is still open, and closes now.
    await expect(draw).toBeVisible();
    await toBody(page);
    await page.keyboard.press("Enter");
    const p = await waitForProject(id, (p) => p.parts.length === 2, 5000);
    expect(p.notes ?? "").toBe("");
    await app.saved();
    expect(await buildings(page)).toBe(2);
  });

  test("async-answer-status: a status change whose figures come back during the press is saved before the call, never thrown away (HUNT-1101)", async ({ app, page, mcp }) => {
    const id = await readyNotes(app, page, mcp, "R12 stav");
    await dockTab(page, "dock.job").click();
    await expect(statusSelect(page)).toBeVisible();
    const snap = await hold(page, "take_snapshot");
    // The banner's Revert of one change runs Revert all (revert_changes).
    const call = await hold(page, "revert_changes");
    await statusSelect(page).selectOption("offer");
    await expect.poll(() => snap.held(), { timeout: 5000 }).toBe(1);
    // Revert from the banner (the AI tab would take the Job tab's place).
    await bannerRevert(page).click();
    await page.waitForTimeout(300);
    expect(call.held(), "the call waits for the status change").toBe(0);
    snap.release();
    await expect.poll(() => call.held(), { timeout: 5000 }).toBe(1);
    const mid = await loadProject(id);
    expect(mid.job.status, "the status change was saved before the call").toBe("offer");
    expect(mid.result_snapshot).toBeTruthy();
    call.release();
    const after = await waitForProject(id, (p) => !p.notes, 8000);
    await app.saved();
    await page.waitForTimeout(1000);
    const now = await loadProject(id);
    expect(now).toEqual(after);
    expect(now.job.status).toBe("offer");
    expect(now.result_snapshot).toBeTruthy();
    await dockTab(page, "dock.job").click();
    await expect(statusSelect(page)).toHaveValue("offer");
  });

  test("async-answer-store: the card's Store answered during the press is saved before the call (HUNT-1101)", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "R12 uložit");
    await mcp.call("update_job", { id, status: "offer", summary: "Nabídka" });
    const p0 = await loadProject(id);
    delete p0.result_snapshot;
    await bridge("save_project", { id, project: p0 });
    await acceptAll(id);
    await app.open({ prefs: { followAi: false } });
    await app.openJob("R12 uložit");
    await app.saved();
    await mcp.call("update_job", { id, notes: "Od AI", summary: "Poznámka od AI" });
    await waitForProject(id, (p) => p.notes === "Od AI");
    await app.saved();
    await aiShown(page);
    await dockTab(page, "dock.results").click();
    const snap = await hold(page, "take_snapshot");
    // The banner's Revert of one change runs Revert all (revert_changes).
    const call = await hold(page, "revert_changes");
    await page.getByTestId("snap-store").click();
    await expect.poll(() => snap.held(), { timeout: 5000 }).toBe(1);
    await bannerRevert(page).click();
    await page.waitForTimeout(300);
    expect(call.held(), "the call waits for Store").toBe(0);
    snap.release();
    await expect.poll(() => call.held(), { timeout: 5000 }).toBe(1);
    expect((await loadProject(id)).result_snapshot, "Store was saved before the call").toBeTruthy();
    call.release();
    await waitForProject(id, (p) => !p.notes, 8000);
    await app.saved();
    await page.waitForTimeout(1000);
    const now = await loadProject(id);
    expect(now.result_snapshot).toBeTruthy();
    await expect(page.getByTestId("snap-store")).toHaveCount(0);
  });

  test("toast-action: a toast's action pressed while the call is out does nothing and the toast stays; pressed after, it works (UI-1101)", async ({ app, page, mcp }) => {
    const id = await createExample("hip", "R12 toast");
    await mcp.call("update_job", { id, status: "offer", summary: "Nabídka" });
    const stored = (await waitForProject(id, (p) => !!p.result_snapshot)).result_snapshot.roof_area as number;
    await mcp.call("update_job", { id, status: "draft", summary: "Zpět na návrh" });
    await mcp.call("update_edges", { id, changes: [{ index: 0, pitch_deg: 40 }], summary: "Sklon podle řezu" });
    await acceptAll(id);
    await app.open({ prefs: { followAi: false } });
    await app.openJob("R12 toast");
    await app.saved();
    await dockTab(page, "dock.job").click();
    await statusSelect(page).selectOption("offer");
    const toast = app.toasts().filter({ hasText: cs("job.offerKeptTitle") });
    await expect(toast).toBeVisible({ timeout: 10000 });
    await waitForProject(id, (p) => p.job.status === "offer");
    await app.saved();
    await mcp.call("update_job", { id, notes: "Od AI", summary: "Poznámka od AI" });
    await waitForProject(id, (p) => p.notes === "Od AI");
    await app.saved();
    await aiShown(page);
    // The banner's Revert of one change runs Revert all (revert_changes).
    const held = await hold(page, "revert_changes");
    await bannerRevert(page).click();
    await expect.poll(() => held.held(), { timeout: 5000 }).toBe(1);
    await toast.getByRole("button", { name: cs("snap.store") }).click();
    await page.waitForTimeout(400);
    await expect(toast, "the toast stays").toHaveCount(1);
    expect((await loadProject(id)).result_snapshot.roof_area).toBe(stored);
    held.release();
    await waitForProject(id, (p) => !p.notes, 8000);
    await app.saved();
    await page.waitForTimeout(800);
    expect((await loadProject(id)).result_snapshot.roof_area, "nothing of the press landed").toBe(stored);
    // Its timer was held: it is still there to be pressed now.
    await expect(toast).toHaveCount(1);
    await toast.getByRole("button", { name: cs("snap.store") }).click();
    await waitForProject(id, (p) => Math.abs(p.result_snapshot.roof_area - stored) > 0.01, 8000);
  });

  test("settings-key: Ctrl/Cmd+, opens no settings while the call is out (HUNT-1103)", async ({ app, page, mcp }) => {
    const id = await readyNotes(app, page, mcp, "R12 nastavení");
    await dockTab(page, "dock.ai").click();
    const held = await hold(page, "revert_last_change");
    await historyRevert(page).click();
    await expect.poll(() => held.held(), { timeout: 5000 }).toBe(1);
    await toBody(page);
    await page.keyboard.press("ControlOrMeta+Comma");
    const end = Date.now() + 600;
    while (Date.now() < end) {
      expect(await page.getByRole("dialog").count(), "a dialog opened during the call").toBe(0);
      await page.waitForTimeout(30);
    }
    held.release();
    await waitForProject(id, (p) => !p.notes, 8000);
    await app.saved();
    // Afterwards the shortcut works again.
    await page.keyboard.press("ControlOrMeta+Comma");
    await expect(page.getByRole("dialog")).toHaveCount(1);
  });

  test("leave-open: the follow offer's Open does not leave while the call is out, and a busy answer names the job (ERR-1105)", async ({ app, page, mcp }) => {
    const other = await createExample("hip", "R12 jiná");
    const id = await readyNotes(app, page, mcp, "R12 odchod", { followAi: true });
    await dockTab(page, "dock.ai").click();
    const busy = await holdBusy(page, "revert_last_change", id);
    await historyRevert(page).click();
    await expect.poll(() => busy.held(), { timeout: 5000 }).toBe(1);
    await mcp.call("show_project", { id: other });
    const offer = app.toasts().filter({ hasText: cs("ai.followOpen") });
    await expect(offer).toHaveCount(1, { timeout: 5000 });
    await offer.getByRole("button", { name: cs("ai.followOpen") }).click();
    await page.waitForTimeout(400);
    await expect(page.locator(".editor header input.title")).toHaveValue("R12 odchod");
    await expect(offer, "the offer stays").toHaveCount(1);
    busy.release();
    const note = app.toasts().filter({ hasText: cs("err.ofJob", { name: "R12 odchod", title: cs("err.action.revert") }) });
    await expect(note).toHaveCount(1);
    await expect(note).toContainText(cs("err.busy"));
    expect((await loadProject(id)).notes).toBe("Od AI");
    // Now the offer leaves.
    await offer.getByRole("button", { name: cs("ai.followOpen") }).click();
    await expect(page.locator(".editor header input.title")).toHaveValue("R12 jiná");
    await expect(note, "the notice outlives the editor it names").toHaveCount(1);
    app.errors.length = 0;
  });

  for (const call of ["revert", "revert-all", "put-back"] as const) {
    test(`ai-save: an AI save landing while the ${call} answer is held is on screen after it (TEST-1101)`, async ({ app, page, mcp }) => {
      const name = `R12 AI ${call}`;
      let id: string;
      if (call === "put-back") {
        id = await olderOffer(mcp, name);
        await app.open({ prefs: { followAi: false } });
        await app.openJob(name);
        await expect(app.toasts().filter({ hasText: "Tectora 0.8.0" })).toHaveCount(1);
        await app.saved();
      } else id = await readyNotes(app, page, mcp, name);
      const cmd = call === "revert" ? "revert_last_change" : call === "revert-all" ? "revert_changes" : "restore_offer_result";
      const held = await hold(page, cmd);
      if (call === "revert") {
        await dockTab(page, "dock.ai").click();
        await historyRevert(page).click();
      } else if (call === "revert-all") await bannerRevert(page).click();
      else {
        await dockTab(page, "dock.results").click();
        await page.getByTestId("snap-restore").click();
      }
      await expect.poll(() => held.held(), { timeout: 5000 }).toBe(1);
      await waitForProject(id, (p) => (call === "put-back" ? p.result_snapshot?.engine_version === "0.8.0" : !p.notes));
      await mcp.call("update_job", { id, customer: "Zákazník od AI", summary: "Zákazník podle výkresu" });
      await waitForProject(id, (p) => p.job.customer === "Zákazník od AI");
      // The watcher's event reaches the editor while the answer is held.
      await page.waitForTimeout(1200);
      held.release();
      await expect(status(page)).not.toHaveText(cs(call === "put-back" ? "editor.restoringOffer" : "editor.reverting"));
      await app.saved();
      await page.waitForTimeout(1200);
      await dockTab(page, "dock.job").click();
      await expect(customer(page)).toHaveValue("Zákazník od AI", { timeout: 5000 });
      const file = await loadProject(id);
      expect(file.job.customer).toBe("Zákazník od AI");
      // The banner lists the AI's new change.
      await expect(page.locator(".review")).toContainText("Zákazník od AI");
    });
  }

  test("presave-held-lock: a save waiting for another writer's lock makes the press answer busy within about 3 s, and the editor stays usable (ERR-1102)", async ({ app, page, mcp }) => {
    const id = await readyNotes(app, page, mcp, "R12 zámek");
    await selectWall(page, 0);
    await dockTab(page, "dock.ai").click();
    const reverts: number[] = [];
    page.on("request", (r) => r.url().endsWith("/revert_last_change") && reverts.push(Date.now()));
    const lock = await holdLock(id, 60_000);
    try {
      await pitch(page).fill("33");
      await pitch(page).press("Enter");
      await expect(status(page)).not.toHaveText(cs("editor.saved"));
      const pressed = Date.now();
      await historyRevert(page).click();
      const note = app.toasts().filter({ hasText: cs("err.ofJob", { name: "R12 zámek", title: cs("err.action.revert") }) });
      await expect(note).toHaveCount(1, { timeout: 4500 });
      expect(Date.now() - pressed, "answered within about the core's lock wait").toBeLessThan(4500);
      // The editor's own save waited: the prepare phase's notice (round 13
      // design A, UX-1201), which names another program saving the job as
      // one of its causes; `err.busy` is the core's own lock answer.
      await expect(note).toContainText(cs("err.busy_edits"));
      expect(reverts, "the call never went out").toEqual([]);
      // Usable meanwhile: not "reverting", the bar and the Back button take input.
      await expect(status(page)).not.toHaveText(cs("editor.reverting"));
      await expect(pitch(page)).toBeEditable();
      expect(await page.locator(".editor header").evaluate((h) => (h as HTMLElement).inert)).toBe(false);
    } finally {
      lock.kill();
    }
    // The late save lands as the user's own edit; nothing was reverted.
    const p = await waitForProject(id, (p) => p.parts[0].edges[0].pitch_deg === 33, 8000);
    expect(p.notes).toBe("Od AI");
    await app.saved();
    await expect(historyRevert(page)).toHaveCount(1);
    app.errors.length = 0;
  });
});

test.describe("round 12: the review banner (ERR-1103, UI-1103) and the put-back's words (UX-1102)", () => {
  test("Accept under another writer's lock keeps the banner until the core answers, and says so under Accept's title (ERR-1103)", async ({ app, page, mcp }) => {
    const id = await readyNotes(app, page, mcp, "R12 přijmout");
    const busy = await holdBusy(page, "accept_changes", id);
    await bannerAccept(page).click();
    await expect.poll(() => busy.held(), { timeout: 5000 }).toBe(1);
    await page.waitForTimeout(300);
    await expect(page.locator(".review"), "the banner stays while the core decides").toHaveCount(1);
    await expect(bannerAccept(page)).toHaveAttribute("aria-busy", "true");
    busy.release();
    const note = app.toasts().filter({ hasText: cs("err.ofJob", { name: "R12 přijmout", title: cs("err.action.accept") }) });
    await expect(note).toHaveCount(1);
    await expect(app.toasts().filter({ hasText: cs("err.action.save") })).toHaveCount(0);
    await expect(page.locator(".review")).toHaveCount(1);
    await expect(bannerAccept(page)).toHaveAttribute("title", cs("review.accept"));
    app.errors.length = 0;
  });

  test("the banner's Revert pressed from the keyboard keeps the focus on it while it waits and after a busy answer (UI-1103)", async ({ app, page, mcp }) => {
    const id = await readyNotes(app, page, mcp, "R12 fokus");
    const busy = await holdBusy(page, "revert_changes", id);
    await keyboardFocus(page, bannerRevert(page));
    await page.keyboard.press("Enter");
    await expect.poll(() => busy.held(), { timeout: 5000 }).toBe(1);
    await expect(bannerRevert(page)).toBeFocused();
    // The banner takes no press while the call is out.
    await page.keyboard.press("Enter");
    await expect(bannerRevert(page)).toHaveAttribute("aria-busy", "true");
    busy.release();
    await expect(app.toasts().filter({ hasText: cs("err.busy") })).toHaveCount(1);
    await expect(bannerRevert(page)).toBeFocused();
    expect(busy.held(), "the second Enter sent nothing").toBe(1);
    app.errors.length = 0;
  });

  test("the banner's Accept pressed from the keyboard leaves the focus in the editor, not on the page (UI-1103)", async ({ app, page, mcp }) => {
    const id = await readyNotes(app, page, mcp, "R12 fokus přijmout");
    void id;
    await keyboardFocus(page, bannerAccept(page));
    await page.keyboard.press("Enter");
    await expect(page.locator(".review")).toHaveCount(0);
    await expect.poll(() => page.evaluate(() => document.activeElement !== document.body && !!document.activeElement)).toBe(true);
  });

  test("a busy put-back is titled as the put-back and the status says putting back (UX-1102)", async ({ app, page, mcp }) => {
    const id = await olderOffer(mcp, "R12 vrátit čísla");
    await app.open({ prefs: { followAi: false } });
    await app.openJob("R12 vrátit čísla");
    await expect(app.toasts().filter({ hasText: "Tectora 0.8.0" })).toHaveCount(1);
    await app.saved();
    await dockTab(page, "dock.results").click();
    const busy = await holdBusy(page, "restore_offer_result", id);
    await page.getByTestId("snap-restore").click();
    await expect.poll(() => busy.held(), { timeout: 5000 }).toBe(1);
    await expect(status(page)).toHaveText(cs("editor.restoringOffer"));
    busy.release();
    await expect(app.toasts().filter({ hasText: cs("err.ofJob", { name: "R12 vrátit čísla", title: cs("err.action.restoreOffer") }) })).toHaveCount(1);
    await expect(app.toasts().filter({ hasText: cs("err.action.revert") })).toHaveCount(0);
    app.errors.length = 0;
  });

  test("the export of a recalculated offer warns, names the job and the way to send the offer's figures (UX-1101)", async ({ app, page, mcp }) => {
    await olderOffer(mcp, "R12 export");
    await app.open({ prefs: { followAi: false } });
    await app.openJob("R12 export");
    await expect(app.toasts().filter({ hasText: "Tectora 0.8.0" })).toHaveCount(1);
    await app.saved();
    fs.mkdirSync(FILES_DIR, { recursive: true });
    const file = path.join(FILES_DIR, "r12-export.tectorajob");
    await dockTab(page, "dock.job").click();
    await app.answerFileDialog(file);
    await page.getByRole("button", { name: cs("files.export") }).click();
    const warn = app.toasts().filter({ hasText: cs("snap.exportRecalculated", { tab: cs("dock.results"), button: cs("snap.revertRefresh") }) });
    await expect(warn).toHaveCount(1);
    await expect(warn).toHaveClass(/warning/);
    await expect(warn).toContainText(cs("snap.titleOf", { name: "R12 export" }));
    // Longer than an info's 4 s.
    await page.waitForTimeout(5000);
    await expect(warn).toHaveCount(1);
  });
});
