// Guards for the round-10 audit (2026-11-23), the editor's half:
//
// ERR-901  an edit made while Revert or the put-back of the offer's figures
//          is with the core (behind another process's lock on the job) is
//          kept and saved, with the autosave before and after the answer
// ERR-902  leaving an offer while its stored result is recalculated: the
//          next open of the job shows the notice it missed
// UX-901   a load warning is shown only in the editor that asked, and names
//          the job
// UI-901   the put-back button fits its label in every language and size
// UI-902   after a put-back / Revert from the keyboard the focus is on the
//          history list, not the page
// UI-903   the zoom buttons' tooltips name their keys
// HUNT-908 no key shortcut acts while the job is still being read
// TEST-904 after the put-back the AI change below it still offers Revert
import { spawn, type ChildProcess } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import type { Locator, Page } from "@playwright/test";
import { LIBRARY_DIR } from "./env";
import { bridge, createExample, expect, loadProject, test, waitForProject, type Locale } from "./fixtures";
import { LOCALES, cs, tr } from "./i18n";

// TECTORA_SHOTS=<folder>/ saves a screenshot after each test (fix evidence).
test.afterEach(async ({ page }, info) => {
  const dir = process.env.TECTORA_SHOTS;
  if (!dir) return;
  const id = info.title.match(/(HUNT|UX|UI|ERR|TEST)-\d+/)?.[0] ?? "r10";
  const n = info.title.replace(/[^a-z0-9]+/gi, "-").slice(0, 40);
  await page.screenshot({ path: `${dir}r10-${id}-${n}-${process.env.TECTORA_SHOTS_TAG ?? "after"}.png`, timeout: 10_000 }).catch(() => {});
});

const dockTab = (page: Page, key: string) => page.locator("aside .tabs").getByRole("tab", { name: cs(key) });
const entries = (page: Page) => page.getByTestId("history-entry");
const putBack = (page: Page, locale: Locale = "cs") => page.getByRole("button", { name: tr(locale, "snap.revertRefresh") });
const recalcToast = (app: { toasts(): Locator }) => app.toasts().filter({ hasText: "Tectora 0.8.0" });
const customer = (page: Page) => page.locator("aside .job").getByLabel(cs("job.customer"));

/** 12 November 2026, the day the "offer" was made. */
const OFFER_MS = Date.UTC(2026, 10, 12, 10, 0, 0);

/**
 * An offer job whose stored result looks like one engine 0.8.0 stored (as
 * audit-r9-recalc `olderOffer`): roof area `stored` m², taken on OFFER_MS,
 * its origin checked by 0.8.0 and the AI's status change accepted.
 */
async function olderOffer(mcp: any, name: string, stored = 150): Promise<string> {
  const id = await createExample("gable", name);
  await mcp.call("update_job", { id, status: "offer", summary: "Offer sent." });
  const file = path.join(LIBRARY_DIR, `${id}.tectora`);
  const p = JSON.parse(fs.readFileSync(file, "utf8"));
  const origins = path.join(LIBRARY_DIR, ".activity", `${id}.result.json`);
  const o = JSON.parse(fs.readFileSync(origins, "utf8")) as { taken_ms: number; checked: string }[];
  for (const x of o) {
    if (x.taken_ms === p.result_snapshot.taken_ms) x.taken_ms = OFFER_MS;
    x.checked = "0.8.0";
  }
  p.result_snapshot.engine_version = "0.8.0";
  p.result_snapshot.roof_area = stored;
  p.result_snapshot.taken_ms = OFFER_MS;
  fs.writeFileSync(file, JSON.stringify(p));
  fs.writeFileSync(origins, JSON.stringify(o));
  await acceptAll(id);
  return id;
}

/** Accepts every change in the job's log (no review banner). */
async function acceptAll(id: string) {
  const log = await bridge<{ ts_ms: number }[]>("project_activity", { id });
  await bridge("accept_changes", { id, tsMs: log.reduce((m, a) => Math.max(m, a.ts_ms), 0) });
}

/**
 * Another process (a slow AI save, a second Tectora on a shared folder)
 * holds the job's lock file for `ms`: every write of the job waits for it.
 * Resolves once the lock is taken. Python's flock, as the audit's
 * reproducer; the E2E suite runs on Linux.
 */
async function holdLock(id: string, ms: number): Promise<ChildProcess> {
  const file = path.join(LIBRARY_DIR, ".activity", `${id}.lock`);
  const h = spawn(
    "python3",
    ["-I", "-c", "import fcntl,sys,time\nf=open(sys.argv[1],'a')\nfcntl.flock(f,fcntl.LOCK_EX)\nprint('held',flush=True)\ntime.sleep(float(sys.argv[2]))", file, String(ms / 1000)],
    { stdio: ["ignore", "pipe", "inherit"] },
  );
  await new Promise<void>((ok, fail) => {
    h.stdout!.once("data", () => ok());
    h.once("error", fail);
    h.once("exit", (code) => fail(new Error(`lock holder exited with ${code}`)));
  });
  return h;
}

/** Holds the bridge's answers to `cmd` (after the core ran it) until released. */
async function hold(page: Page, cmd: string, when: (body: any) => boolean = () => true): Promise<{ release: () => void; held: () => number }> {
  let open!: () => void;
  const gate = new Promise<void>((r) => (open = r));
  let n = 0;
  await page.route(`**/${cmd}`, async (route) => {
    let body: any = {};
    try {
      body = JSON.parse(route.request().postData() ?? "{}");
    } catch {
      // Not JSON: not ours to hold.
    }
    if (!when(body)) return route.continue();
    const res = await route.fetch();
    n++;
    await gate;
    await route.fulfill({ response: res }).catch(() => {});
  });
  return { release: open, held: () => n };
}

/** Times (ms since the page's start) the editor sent requests to `cmd` and got their answers. */
function timeline(page: Page, cmd: string) {
  const sent: number[] = [];
  const answered: number[] = [];
  page.on("request", (r) => {
    if (r.url().endsWith(`/${cmd}`)) sent.push(Date.now());
  });
  page.on("requestfinished", (r) => {
    if (r.url().endsWith(`/${cmd}`)) answered.push(Date.now());
  });
  return { sent, answered };
}

/** Types `text` into the job's customer field and leaves it (the value is stored then). */
async function typeCustomer(page: Page, text: string) {
  await dockTab(page, "dock.job").click();
  await customer(page).click();
  await customer(page).press("ControlOrMeta+a");
  await page.keyboard.type(text);
  await page.keyboard.press("Tab");
}

test.describe("round 10: an edit made while Revert or the put-back is with the core stays (ERR-901)", () => {
  test.skip(process.platform === "win32", "the lock holder uses Python's fcntl");

  /** An offer recalculated on open, the editor on its AI tab with the put-back button. */
  async function putBackReady(app: any, page: Page, mcp: any, name: string) {
    const id = await olderOffer(mcp, name);
    await app.open({ prefs: { followAi: false } });
    await app.openJob(name);
    await expect(recalcToast(app)).toHaveCount(1);
    await app.saved();
    await dockTab(page, "dock.ai").click();
    await expect(putBack(page)).toHaveCount(1);
    return id;
  }

  /** A draft with an AI's notes change the editor shows with Revert on it. */
  async function revertReady(app: any, page: Page, mcp: any, name: string) {
    const id = await createExample("gable", name);
    await app.open({ prefs: { followAi: false } });
    await app.openJob(name);
    await app.saved();
    await mcp.call("update_job", { id, notes: "Od AI", summary: "Poznámka od AI" });
    await waitForProject(id, (p) => p.notes === "Od AI");
    await dockTab(page, "dock.ai").click();
    const revert = entries(page).first().getByRole("button", { name: cs("ai.revert") });
    await expect(revert).toHaveCount(1);
    await expect(page.locator(".editor textarea, .editor input").first()).toBeVisible();
    return { id, revert };
  }

  for (const action of ["put-back", "revert"] as const) {
    test(`${action}: the autosave of an edit typed during the call is sent before the answer, and the edit is saved (ERR-901)`, async ({ app, page, mcp }) => {
      let id: string;
      let press: () => Promise<void>;
      if (action === "put-back") {
        id = await putBackReady(app, page, mcp, `Zámek ${action}`);
        press = () => putBack(page).click();
      } else {
        const r = await revertReady(app, page, mcp, `Zámek ${action}`);
        id = r.id;
        press = () => r.revert.click();
      }
      const cmd = action === "put-back" ? "restore_offer_result" : "revert_last_change";
      const call = timeline(page, cmd);
      // A save's first question (is the jobs folder still the same?): the autosave has begun.
      const saves = timeline(page, "projects_dir");
      // Held until the autosave is seen (the finally below), not for a fixed
      // time: on a slow runner typing alone took longer than 3 s.
      const lock = await holdLock(id, 20_000);
      try {
        await press();
        await typeCustomer(page, "Novák během");
        // The edit's autosave begins while the core still waits for the lock.
        await expect.poll(() => saves.sent.length, { timeout: 2500 }).toBeGreaterThan(0);
        await expect(page.locator(".statusbar .save")).toHaveText(cs("editor.saving"));
        expect(call.answered, "the call is still out when the autosave begins").toHaveLength(0);
      } finally {
        lock.kill();
      }
      const done = await waitForProject(id, (p) => p.job?.customer === "Novák během" && (action === "put-back" ? p.result_snapshot?.engine_version === "0.8.0" : !p.notes), 12_000);
      expect(call.answered.length).toBe(1);
      if (action === "put-back") expect(done.result_snapshot.roof_area).toBe(150);
      await app.saved();
      await expect(customer(page)).toHaveValue("Novák během");
      expect((await loadProject(id)).job.customer).toBe("Novák během");
    });

    test(`${action}: an edit typed during the call whose autosave runs after the answer is saved (ERR-901)`, async ({ app, page, mcp }) => {
      let id: string;
      let press: () => Promise<void>;
      if (action === "put-back") {
        id = await putBackReady(app, page, mcp, `Po odpovědi ${action}`);
        press = () => putBack(page).click();
      } else {
        const r = await revertReady(app, page, mcp, `Po odpovědi ${action}`);
        id = r.id;
        press = () => r.revert.click();
      }
      const cmd = action === "put-back" ? "restore_offer_result" : "revert_last_change";
      const saves = timeline(page, "save_project_checked");
      // The core waits for the lock; its answer then waits for the edit.
      const answer = await hold(page, cmd);
      const lock = await holdLock(id, 1200);
      try {
        await press();
        await expect.poll(() => answer.held(), { timeout: 8000 }).toBe(1);
      } finally {
        lock.kill();
      }
      await typeCustomer(page, "Novák potom");
      const before = saves.sent.length;
      answer.release();
      // The answer is in before the edit's autosave (0.5 s after the edit) leaves.
      await expect(page.locator(".statusbar .save")).not.toHaveText(/Uloženo/, { timeout: 400 });
      const done = await waitForProject(id, (p) => p.job?.customer === "Novák potom", 8000);
      expect(saves.sent.length).toBeGreaterThan(before);
      if (action === "put-back") expect(done.result_snapshot).toMatchObject({ engine_version: "0.8.0", roof_area: 150 });
      else expect(done.notes ?? "").toBe("");
      await app.saved();
      await expect(customer(page)).toHaveValue("Novák potom");
    });
  }
});

test.describe("round 10: notices and warnings of the job being opened (ERR-902, UX-901)", () => {
  test("leaving an offer while its stored result is recalculated: the next open of the job shows the notice (ERR-902)", async ({ app, page, mcp }) => {
    const id = await olderOffer(mcp, "Odchod");
    const held = await hold(page, "refresh_stored_result", (b) => b.id === id);
    await app.open({ prefs: { followAi: false } });
    await page.getByRole("row").filter({ hasText: "Odchod" }).first().dblclick();
    await expect.poll(() => held.held()).toBe(1);
    await page.getByRole("button", { name: cs("editor.back") }).first().click();
    const answered = page.waitForResponse((r) => r.url().endsWith("/refresh_stored_result"));
    held.release();
    await answered;
    // Not in the library.
    await page.waitForTimeout(300);
    await expect(recalcToast(app)).toHaveCount(0);
    expect((await loadProject(id)).result_snapshot.engine_version).not.toBe("0.8.0");
    // The next open has nothing to recalculate, and still says what the first one did.
    await app.openJob("Odchod");
    await expect(recalcToast(app)).toHaveCount(1);
    await expect(recalcToast(app)).toContainText("150");
    await app.saved();
    // Told once: leaving and opening again says nothing more.
    await page.getByRole("button", { name: cs("editor.back") }).first().click();
    await expect(recalcToast(app)).toHaveCount(0, { timeout: 8000 });
    await app.openJob("Odchod");
    await app.saved();
    await page.waitForTimeout(500);
    await expect(recalcToast(app)).toHaveCount(0);
  });

  /** A job whose stored result has an impossible time: the app opens it without the result and warns. */
  function impossibleDate(id: string) {
    const file = path.join(LIBRARY_DIR, `${id}.tectora`);
    const p = JSON.parse(fs.readFileSync(file, "utf8"));
    p.result_snapshot = { engine_version: "0.9.0", taken_ms: 4242424242, roof_area: 99, ridge_height: 5, lines_by_code: { T: 20 } };
    fs.writeFileSync(file, JSON.stringify(p).replace("4242424242", "8640000000000001"));
  }

  test("a load warning is shown in the editor that asked, names the job, and never in the library or the next job (UX-901)", async ({ app, page }) => {
    const id = await createExample("gable", "Nemožné datum");
    await createExample("hip", "Jiná");
    impossibleDate(id);
    const warning = app.toasts().filter({ hasText: cs("err.result_dropped") });

    // Asked, then left before the answer: nothing in the library, nothing in the next job.
    const held = await hold(page, "load_warnings", (b) => b.id === id);
    await app.open({ prefs: { followAi: false } });
    await page.getByRole("row").filter({ hasText: "Nemožné datum" }).first().dblclick();
    await expect.poll(() => held.held()).toBe(1);
    await page.getByRole("button", { name: cs("editor.back") }).first().click();
    await app.openJob("Jiná");
    await app.saved();
    const answered = page.waitForResponse((r) => r.url().endsWith("/load_warnings"));
    held.release();
    await answered;
    await page.waitForTimeout(500);
    await expect(warning).toHaveCount(0);
    await page.getByRole("button", { name: cs("editor.back") }).first().click();
    await page.waitForTimeout(300);
    await expect(warning).toHaveCount(0);

    // Its own editor says it, with the job's name.
    await app.openJob("Nemožné datum");
    await expect(warning).toHaveCount(1);
    await expect(warning).toContainText(cs("snap.titleOf", { name: "Nemožné datum" }));
    await expect(warning).toContainText("result_dropped");
  });
});

test.describe("round 10: the history's buttons (UI-901, UI-902, TEST-904)", () => {
  for (const locale of LOCALES) {
    test(`the put-back button holds its label inside its border at 1360x860, 800x600 and 600x420, ${locale} (UI-901)`, async ({ app, page, mcp }) => {
      await olderOffer(mcp, "Popisek");
      const sizes = [
        { width: 1360, height: 860 },
        { width: 800, height: 600 },
        { width: 600, height: 420 },
      ];
      await app.open({ locale, prefs: { followAi: false } });
      await app.openJob("Popisek");
      await expect(recalcToast(app)).toHaveCount(1);
      for (const size of sizes) {
        await page.setViewportSize(size);
        // The panel is opened from the header's AI button, which is there at every width.
        await page.locator(".ai-pill").click();
        const btn = putBack(page, locale);
        await expect(btn).toBeVisible();
        await btn.scrollIntoViewIfNeeded();
        const fit = await btn.evaluate((el) => {
          const box = el.getBoundingClientRect();
          const range = document.createRange();
          range.selectNodeContents(el);
          const text = range.getBoundingClientRect();
          return {
            over: el.scrollHeight - el.clientHeight,
            wide: el.scrollWidth - el.clientWidth,
            top: text.top - box.top,
            bottom: box.bottom - text.bottom,
            right: box.right - text.right,
          };
        });
        const where = `${locale} ${size.width}x${size.height}`;
        expect(fit.over, `${where}: the label is taller than the button`).toBeLessThanOrEqual(0);
        expect(fit.wide, `${where}: the label is wider than the button`).toBeLessThanOrEqual(0);
        expect(fit.top, `${where}: the label runs over the top border`).toBeGreaterThanOrEqual(1);
        expect(fit.bottom, `${where}: the label runs over the bottom border`).toBeGreaterThanOrEqual(1);
        expect(fit.right, `${where}: the label runs over the right border`).toBeGreaterThanOrEqual(0);
      }
    });
  }

  test("after the put-back from the keyboard the focus is on the history, and the AI change below it still offers Revert (UI-902, TEST-904)", async ({ app, page, mcp }) => {
    const id = await olderOffer(mcp, "Klávesnice");
    await app.open({ prefs: { followAi: false } });
    await app.openJob("Klávesnice");
    await expect(recalcToast(app)).toHaveCount(1);
    await app.saved();
    // An AI note after the recalculation: the put-back's entry will be on top of it.
    await mcp.call("update_job", { id, notes: "Zavolat zákazníkovi", summary: "Poznámka" });
    await waitForProject(id, (p) => p.notes === "Zavolat zákazníkovi");
    await dockTab(page, "dock.ai").click();
    await expect(entries(page).first()).toContainText("Poznámka");

    await putBack(page).focus();
    await page.keyboard.press("Enter");
    await waitForProject(id, (p) => p.result_snapshot?.engine_version === "0.8.0");
    await expect(putBack(page)).toHaveCount(0);
    const history = page.locator("aside ol.history");
    await expect(history).toBeFocused();

    // TEST-904: the user's put-back is the newest entry and has no Revert; the AI's note under it has.
    await expect(entries(page).first()).toContainText(cs("chg.resultRestored").split(" (")[0]);
    await expect(entries(page).first().getByRole("button", { name: cs("ai.revert") })).toHaveCount(0);
    const revert = entries(page).nth(1).getByRole("button", { name: cs("ai.revert") });
    await expect(revert).toHaveCount(1);

    // UI-902 for Revert: pressed from the keyboard, the focus lands on the history too.
    await revert.focus();
    await page.keyboard.press("Enter");
    const back = await waitForProject(id, (p) => !p.notes);
    expect(back.result_snapshot.engine_version).toBe("0.8.0");
    await expect(revert).toHaveCount(0);
    await expect(history).toBeFocused();
  });
});

test.describe("round 10: the editor's keys and tooltips (HUNT-908, UI-903)", () => {
  for (const key of ["Insert", "Control+b"]) {
    test(`${key} pressed while the job is being read does nothing once it is in (HUNT-908)`, async ({ app, page }) => {
      const id = await createExample("gable", `Klávesy ${key}`);
      const held = await hold(page, "refresh_stored_result", (b) => b.id === id);
      await app.open({ prefs: { followAi: false } });
      await page.getByRole("row").filter({ hasText: `Klávesy ${key}` }).first().dblclick();
      await expect.poll(() => held.held()).toBe(1);
      await expect(page.getByTestId("editor-loading")).toBeVisible();
      await page.locator("body").press(key);
      held.release();
      await expect(page.locator(".editor .statusbar")).toBeVisible();
      await app.saved();
      await page.waitForTimeout(300);
      await expect(page.getByText(cs("draw.title"))).toHaveCount(0);
      await expect(page.getByText(cs("draw.start"))).toHaveCount(0);
      // The same key once the job is in still starts the footprint entry.
      await page.locator("body").press(key);
      await expect(page.getByText(cs("draw.start"))).toBeVisible();
    });
  }

  for (const locale of LOCALES) {
    test(`the zoom buttons' tooltips name their keys, ${locale} (UI-903)`, async ({ app, page }) => {
      await createExample("gable", "Lupa");
      await app.open({ locale, prefs: { followAi: false } });
      await app.openJob("Lupa");
      await expect(page.getByRole("button", { name: tr(locale, "plan.zoomIn"), exact: true })).toHaveAttribute("title", `${tr(locale, "plan.zoomIn")} (+)`);
      await expect(page.getByRole("button", { name: tr(locale, "plan.zoomOut"), exact: true })).toHaveAttribute("title", `${tr(locale, "plan.zoomOut")} (−)`);
    });
  }
});
