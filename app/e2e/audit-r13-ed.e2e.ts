// Guards for the round-13 audit (2026-11-23), the editor's half.
//
// Design A: the prepare phase of Revert / the put-back ends only on a quiet
// editor (ERR-1201, UI-1201, ERR-1203, HUNT-1204, UX-1201 / HUNT-1202,
// TEST-1202). The earlier edit (a status change) is held at the browser, so
// the press waits in its prepare phase; the intruder acts then, before the
// claim, and the answer is released afterwards. The E2E run is the dev build:
// a commit that reached the backstop (`CoreAction.foreign`) would throw and
// fail the test on its page error.
//   prepare-typing      a value typed (no Enter) holds the claim: kept in its field, not stored, the press ends busy_edits; with Enter it is saved and the revert runs
//   prepare-half-typed  "0," in Overhang is never stored as 0
//   prepare-menu        a context menu opened then holds the claim; its entry runs as a normal edit, then the revert
//   prepare-dialog      the object dialog opened then holds the claim; Cancel afterwards leaves Undo working
//   prepare-confirm     a delete asked then (async confirm) holds the claim; answered after the deadline it is a normal edit
//   prepare-leave       Back pressed then leaves after the press, and the revert is in the file
//   own-slow-edit       the earlier edit with the core past the deadline, no lock: the notice is busy_edits, not busy
// Design B: an AI's folder request allowed is a way out like Back (HUNT-1201).
// Design E: the banner's Revert says what the core undid (HUNT-1205).
// UI-1202: the "kept its earlier result" toast closes once the stored result changes.
import type { Page } from "@playwright/test";
import fs from "node:fs";
import path from "node:path";
import { bar, clickPlan, selectWall, tool } from "./editor";
import { E2E_DIR, LIBRARY_DIR } from "./env";
import { bridge, createExample, expect, loadProject, test, waitForProject } from "./fixtures";
import { cs } from "./i18n";
import { acceptAll, hold } from "./review";

// TECTORA_SHOTS=<folder>/ saves a screenshot after each test (fix evidence).
test.afterEach(async ({ page }, info) => {
  const dir = process.env.TECTORA_SHOTS;
  if (!dir) return;
  const id = info.title.match(/(HUNT|UX|UI|ERR|TEST)-\d+/)?.[0] ?? "r13";
  const n = info.title.replace(/[^a-z0-9]+/gi, "-").slice(0, 40);
  await page.screenshot({ path: `${dir}r13-${id}-${n}-${process.env.TECTORA_SHOTS_TAG ?? "after"}.png`, timeout: 10_000 }).catch(() => {});
});

const dockTab = (page: Page, key: string) => page.locator("aside .tabs").getByRole("tab", { name: cs(key) });
const statusSelect = (page: Page) => page.locator("aside .job select").first();
const bannerRevert = (page: Page) => page.locator(".review").getByRole("button", { name: cs("ai.revert") });
const pitch = (page: Page) => bar(page, "def.title").getByLabel(cs("edge.pitch"));
const overhang = (page: Page) => bar(page, "def.title").getByLabel(cs("edge.overhang"));
const back = (page: Page) => page.getByRole("button", { name: cs("editor.back") }).first();
const revertNote = (app: any, name: string) => app.toasts().filter({ hasText: cs("err.ofJob", { name, title: cs("err.action.revert") }) });

/** Requests the editor sent to `cmd`. */
function sent(page: Page, cmd: string): number[] {
  const at: number[] = [];
  page.on("request", (r) => r.url().endsWith(`/${cmd}`) && at.push(Date.now()));
  return at;
}

/** A gable job open in the editor with one AI change (the notes) pending, wall 1 in the bar. */
async function readyNotes(app: any, page: Page, mcp: any, name: string) {
  const id = await createExample("gable", name);
  await app.open({ prefs: { followAi: false } });
  await app.openJob(name);
  await app.saved();
  await mcp.call("update_job", { id, notes: "Od AI", summary: "Poznámka od AI" });
  await waitForProject(id, (p) => p.notes === "Od AI");
  await app.saved();
  await expect(page.locator(".review")).toBeVisible();
  await selectWall(page, 0);
  return id;
}

/**
 * The earlier edit: the status goes to offer and the core's figures for it
 * are held at the browser; then the banner's Revert is pressed. The press
 * waits in its prepare phase until the hold is released.
 */
async function pressDuringEdit(page: Page) {
  await dockTab(page, "dock.job").click();
  const snap = await hold(page, "take_snapshot");
  await statusSelect(page).selectOption("offer");
  await expect.poll(() => snap.held(), { timeout: 5000 }).toBe(1);
  await bannerRevert(page).click();
  return snap;
}

/** Types like a person into `field`, without Enter. */
async function typeInto(page: Page, field: ReturnType<Page["locator"]>, text: string) {
  await field.click();
  await field.press("ControlOrMeta+a");
  await page.keyboard.type(text);
}

test.describe("round 13: the prepare phase claims only a quiet editor (design A)", () => {
  test("prepare-typing: a value typed without Enter keeps its field and focus, is not stored, and the press ends busy_edits (ERR-1201, UX-1201)", async ({ app, page, mcp }) => {
    const id = await readyNotes(app, page, mcp, "R13 psaní");
    const reverts = sent(page, "revert_changes");
    const snap = await pressDuringEdit(page);
    await typeInto(page, pitch(page), "37");
    snap.release();
    const note = revertNote(app, "R13 psaní");
    await expect(note).toHaveCount(1, { timeout: 6000 });
    await expect(note).toContainText(cs("err.busy_edits"));
    await expect(app.toasts().filter({ hasText: cs("err.busy") })).toHaveCount(0);
    expect(reverts, "the call never went out").toEqual([]);
    await expect(pitch(page)).toHaveValue("37");
    await expect(pitch(page)).toBeFocused();
    const p = await loadProject(id);
    expect(p.parts[0].edges[0].pitch_deg, "not stored half-way").toBe(40);
    expect(p.notes).toBe("Od AI");
    expect(p.job.status, "the earlier edit landed").toBe("offer");
    app.errors.length = 0;
  });

  test("prepare-typing with Enter: the value is saved first, then the revert runs (ERR-1201)", async ({ app, page, mcp }) => {
    const id = await readyNotes(app, page, mcp, "R13 Enter");
    const snap = await pressDuringEdit(page);
    await typeInto(page, pitch(page), "37");
    snap.release();
    await page.waitForTimeout(300);
    await pitch(page).press("Enter");
    const p = await waitForProject(id, (p) => !p.notes, 8000);
    expect(p.parts[0].edges[0].pitch_deg).toBe(37);
    expect(p.job.status).toBe("offer");
    await app.saved();
    await expect(page.locator(".review")).toHaveCount(0);
    expect(app.toasts().filter({ hasText: cs("editor.notMade") })).toHaveCount(0);
  });

  test("prepare-half-typed: '0,' in Overhang is never stored as 0 while the press waits (UI-1201)", async ({ app, page, mcp }) => {
    const id = await readyNotes(app, page, mcp, "R13 přesah");
    const before = (await loadProject(id)).parts[0].edges[0].overhang;
    const snap = await pressDuringEdit(page);
    await typeInto(page, overhang(page), "0,");
    snap.release();
    // Past the passes of the prepare loop: the field is not blurred by them.
    await page.waitForTimeout(1500);
    await expect(overhang(page)).toHaveValue("0,");
    await expect(overhang(page)).toBeFocused();
    expect((await loadProject(id)).parts[0].edges[0].overhang).toBe(before);
    await page.keyboard.type("8");
    await expect(revertNote(app, "R13 přesah")).toHaveCount(1, { timeout: 6000 });
    await overhang(page).press("Enter");
    const p = await waitForProject(id, (p) => p.parts[0].edges[0].overhang === 0.8, 8000);
    expect(p.notes, "nothing was reverted").toBe("Od AI");
    app.errors.length = 0;
  });

  test("prepare-menu: a context menu opened while the press waits holds the call; its entry is a normal edit and the revert follows (ERR-1201)", async ({ app, page, mcp }) => {
    const id = await readyNotes(app, page, mcp, "R13 menu");
    const reverts = sent(page, "revert_changes");
    const snap = await pressDuringEdit(page);
    const box = (await page.locator(".view .wrap > svg g.edge line.hit").nth(0).boundingBox())!;
    await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2, { button: "right" });
    const entry = page.getByRole("menuitem", { name: cs("menu.makeGable") });
    await expect(entry).toBeVisible();
    snap.release();
    await page.waitForTimeout(600);
    expect(reverts, "no call while the menu is open").toEqual([]);
    await entry.click();
    const p = await waitForProject(id, (p) => !p.notes, 8000);
    expect(p.parts[0].edges[0].pitch_deg, "the menu's edit was made").toBe(90);
    await app.saved();
    expect(app.toasts().filter({ hasText: cs("editor.notMade") })).toHaveCount(0);
  });

  test("prepare-dialog: the object dialog opened while the press waits holds the call; Cancel afterwards leaves Undo working (ERR-1201)", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "R13 dialog");
    const p0 = await loadProject(id);
    p0.chimneys = [{ corners: [{ x: 5, y: 1.5 }, { x: 5.6, y: 1.5 }, { x: 5.6, y: 2.1 }, { x: 5, y: 2.1 }], height: 8 }];
    await bridge("save_project", { id, project: p0 });
    await acceptAll(id);
    await app.open({ prefs: { followAi: false } });
    await app.openJob("R13 dialog");
    await app.saved();
    await mcp.call("update_job", { id, notes: "Od AI", summary: "Poznámka od AI" });
    await waitForProject(id, (p) => p.notes === "Od AI");
    await app.saved();
    await expect(page.locator(".review")).toBeVisible();
    // An undo step before the dialog: the pitch.
    await selectWall(page, 0);
    await pitch(page).fill("38");
    await pitch(page).press("Enter");
    await waitForProject(id, (p) => p.parts[0].edges[0].pitch_deg === 38);
    await app.saved();
    const reverts = sent(page, "revert_changes");
    const snap = await pressDuringEdit(page);
    const bb = (await page.locator("path.opening.chimney").first().boundingBox())!;
    await page.mouse.dblclick(bb.x + bb.width / 2, bb.y + bb.height / 2);
    const dlg = page.getByTestId("object-dialog");
    await expect(dlg).toBeVisible();
    snap.release();
    await expect(revertNote(app, "R13 dialog")).toHaveCount(1, { timeout: 6000 });
    await expect(revertNote(app, "R13 dialog")).toContainText(cs("err.busy_edits"));
    expect(reverts).toEqual([]);
    await page.keyboard.press("Escape");
    await expect(dlg).toHaveCount(0);
    await app.saved();
    // The history is whole: Undo takes back the pitch.
    await page.locator("body").press("ControlOrMeta+z");
    await waitForProject(id, (p) => p.parts[0].edges[0].pitch_deg === 40, 8000);
    expect(app.toasts().filter({ hasText: cs("editor.notMade") })).toHaveCount(0);
    app.errors.length = 0;
  });

  test("prepare-confirm: a delete asked while the press waits holds the call; answered after the deadline it is a normal edit (HUNT-1204)", async ({ app, page, mcp }) => {
    const id = await readyNotes(app, page, mcp, "R13 dotaz");
    await page.evaluate(() => {
      const w = window as any;
      w.confirm = (m: string) => {
        w.__e2e.dialogs.push({ kind: "confirm", message: String(m) });
        return new Promise((res) => (w.__answer = res));
      };
    });
    const reverts = sent(page, "revert_changes");
    const snap = await pressDuringEdit(page);
    await tool(page, "tool.clear").click();
    await expect.poll(() => page.evaluate(() => typeof (window as any).__answer)).toBe("function");
    snap.release();
    await expect(revertNote(app, "R13 dotaz")).toHaveCount(1, { timeout: 6000 });
    await expect(revertNote(app, "R13 dotaz")).toContainText(cs("err.busy_edits"));
    expect(reverts).toEqual([]);
    await page.evaluate(() => (window as any).__answer(true));
    const p = await waitForProject(id, (p) => p.parts.length === 0, 8000);
    expect(p.notes).toBe("Od AI");
    expect(app.toasts().filter({ hasText: cs("editor.notMade") })).toHaveCount(0);
    app.errors.length = 0;
  });

  test("prepare-leave: Back pressed while the press waits leaves after it, and the revert is in the file (ERR-1203)", async ({ app, page, mcp }) => {
    const id = await readyNotes(app, page, mcp, "R13 odchod");
    const snap = await pressDuringEdit(page);
    await back(page).click();
    await page.waitForTimeout(300);
    await expect(page.locator(".editor"), "the editor waits for the press").toBeVisible();
    snap.release();
    await expect(page.locator(".editor")).toHaveCount(0, { timeout: 8000 });
    const p = await loadProject(id);
    expect(p.notes ?? "", "the revert was made before leaving").toBe("");
    expect(p.job.status).toBe("offer");
  });

  test("own-slow-edit: the earlier edit still with the core past the deadline gives busy_edits, not the other-program busy (UX-1201, HUNT-1202)", async ({ app, page, mcp }) => {
    const id = await readyNotes(app, page, mcp, "R13 pomalu");
    const reverts = sent(page, "revert_changes");
    const snap = await pressDuringEdit(page);
    const note = revertNote(app, "R13 pomalu");
    await expect(note).toHaveCount(1, { timeout: 4500 });
    await expect(note).toContainText(cs("err.busy_edits"));
    await expect(app.toasts().filter({ hasText: cs("err.busy") })).toHaveCount(0);
    snap.release();
    await waitForProject(id, (p) => p.job.status === "offer", 8000);
    await app.saved();
    expect(reverts).toEqual([]);
    expect((await loadProject(id)).notes).toBe("Od AI");
    app.errors.length = 0;
  });
});

test.describe("round 13: an AI's folder request allowed is a way out like Back (design B, HUNT-1201)", () => {
  async function askFolder(mcp: any, name: string) {
    const other = path.join(E2E_DIR, `r13-${name}`);
    fs.rmSync(other, { recursive: true, force: true });
    fs.mkdirSync(other, { recursive: true });
    await mcp.call("update_settings", { projects_dir: other, summary: "Use the shared folder" });
    return other;
  }
  const request = (page: Page) => page.getByRole("dialog", { name: cs("folderReq.title") });

  test("a failed save: Allow asks to leave; No keeps the editor, the folder and the request", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "R13 složka");
    await app.open();
    await app.openJob("R13 složka");
    await app.saved();
    const before = (await mcp.call("get_settings")).effective.projects_dir;
    await page.route("**/save_project*", (r) => r.fulfill({ contentType: "application/json", body: JSON.stringify({ err: { code: "io" } }) }));
    await selectWall(page, 0);
    await pitch(page).fill("31");
    await pitch(page).press("Enter");
    await expect(page.locator(".statusbar .save-failed")).toBeVisible({ timeout: 8000 });
    await askFolder(mcp, "fail");
    await app.answerConfirm(false);
    await request(page).getByRole("button", { name: cs("folderReq.allow") }).click();
    await expect.poll(async () => (await app.dialogs()).map((d) => d.message)).toContain(cs("editor.leaveUnsaved"));
    await expect(page.locator(".editor")).toBeVisible();
    await expect(request(page)).toHaveCount(1);
    expect((await mcp.call("get_settings")).effective.projects_dir).toBe(before);
    void id;
    app.errors.length = 0;
  });

  test("a footprint being drawn: Allow asks as Back does (UX-013)", async ({ app, page, mcp }) => {
    await createExample("gable", "R13 kresba");
    await app.open();
    await app.openJob("R13 kresba");
    await app.saved();
    await tool(page, "tool.draw").click();
    const draw = bar(page, "draw.title");
    await clickPlan(page, 12.5, 1);
    const len = draw.getByLabel(cs("draw.length"));
    await len.fill("3");
    await len.press("ArrowRight");
    await len.fill("4");
    await len.press("ArrowUp");
    await expect(draw).toContainText(cs("draw.points", { n: 3 }));
    const before = (await mcp.call("get_settings")).effective.projects_dir;
    await askFolder(mcp, "draw");
    await app.answerConfirm(false);
    await request(page).getByRole("button", { name: cs("folderReq.allow") }).click();
    await expect.poll(async () => (await app.dialogs()).map((d) => d.message)).toContain(cs("draw.confirmCancel", { n: 3 }));
    await expect(draw).toContainText(cs("draw.points", { n: 3 }));
    expect((await mcp.call("get_settings")).effective.projects_dir).toBe(before);
  });

  test("a Revert out: the folder moves only after its answer, and the revert is in the old folder's file", async ({ app, page, mcp }) => {
    const id = await readyNotes(app, page, mcp, "R13 složka revert");
    const held = await hold(page, "revert_changes");
    await bannerRevert(page).click();
    await expect.poll(() => held.held(), { timeout: 5000 }).toBe(1);
    const before = (await mcp.call("get_settings")).effective.projects_dir;
    const other = await askFolder(mcp, "revert");
    await request(page).getByRole("button", { name: cs("folderReq.allow") }).click();
    await page.waitForTimeout(500);
    expect((await mcp.call("get_settings")).effective.projects_dir, "not while the call is out").toBe(before);
    held.release();
    await expect.poll(async () => (await mcp.call("get_settings")).effective.projects_dir, { timeout: 8000 }).toBe(other);
    // The old folder's file (the bridge reads the new folder now).
    const file = JSON.parse(fs.readFileSync(path.join(LIBRARY_DIR, `${id}.tectora`), "utf8"));
    expect(file.notes ?? "", "reverted in the folder the job came from").toBe("");
    await expect(app.toasts().filter({ hasText: cs("err.no_such_change") })).toHaveCount(0);
  });
});

test.describe("round 13: what Revert undid (design E) and stale result toasts (UI-1202)", () => {
  test("the banner's Revert of one change says so when an AI change came in before the core took it (HUNT-1205)", async ({ app, page, mcp }) => {
    const id = await readyNotes(app, page, mcp, "R13 navíc");
    // The request waits at the browser, as behind another writer's lock; the AI saves meanwhile.
    let go!: () => void;
    const gate = new Promise<void>((r) => (go = r));
    let n = 0;
    await page.route("**/revert_changes", async (r) => {
      n++;
      await gate;
      await r.continue();
    });
    await bannerRevert(page).click();
    await expect.poll(() => n, { timeout: 5000 }).toBe(1);
    await mcp.call("update_job", { id, customer: "Zákazník od AI", summary: "Zákazník podle výkresu" });
    await waitForProject(id, (p) => p.job.customer === "Zákazník od AI");
    go();
    await waitForProject(id, (p) => !p.notes && !p.job.customer, 8000);
    await expect(app.toasts().filter({ hasText: cs("review.revertedMore", { n: 1 }) })).toHaveCount(1);
  });

  test("the banner's Revert of one change says nothing more when nothing else came in (HUNT-1205)", async ({ app, page, mcp }) => {
    const id = await readyNotes(app, page, mcp, "R13 jen jedna");
    await bannerRevert(page).click();
    await waitForProject(id, (p) => !p.notes, 8000);
    await app.saved();
    await page.waitForTimeout(500);
    await expect(app.toasts()).toHaveCount(0);
  });

  test("the toast that quotes the stored result closes once the stored result changes (UI-1202)", async ({ app, page, mcp }) => {
    const id = await createExample("hip", "R13 toast");
    await mcp.call("update_job", { id, status: "offer", summary: "Nabídka" });
    const stored = (await waitForProject(id, (p) => !!p.result_snapshot)).result_snapshot.roof_area as number;
    await mcp.call("update_job", { id, status: "draft", summary: "Zpět na návrh" });
    await mcp.call("update_edges", { id, changes: [{ index: 0, pitch_deg: 40 }], summary: "Sklon podle řezu" });
    await acceptAll(id);
    await app.open({ prefs: { followAi: false } });
    await app.openJob("R13 toast");
    await app.saved();
    await dockTab(page, "dock.job").click();
    await statusSelect(page).selectOption("offer");
    const toast = app.toasts().filter({ hasText: cs("job.offerKeptTitle") });
    await expect(toast).toBeVisible({ timeout: 10000 });
    await waitForProject(id, (p) => p.job.status === "offer");
    await app.saved();
    // The AI stores the current figures (the card's own Store sits under the toast here):
    // the toast's figures are old now. Hovered, its timer starts again on leaving.
    await toast.hover();
    await page.mouse.move(10, 10);
    await mcp.call("store_result", { id, summary: "Výsledek uložen" });
    await waitForProject(id, (p) => Math.abs(p.result_snapshot.roof_area - stored) > 0.01, 8000);
    await expect(toast, "closed with the stored result it quoted, not by its timer").toHaveCount(0, { timeout: 4000 });
  });
});
