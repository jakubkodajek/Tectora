// Guards for the round-10 audit (2026-11-23), the editor's half:
//
// ERR-901  an edit tried while Revert or the put-back of the offer's figures
//          is with the core (behind another process's lock on the job) is
//          refused, nothing is lost, and the value typed after the answer
//          lands within 3 s (rewritten in round 11 to the owner's decision
//          of design A1: the call is a transaction like a drag; the round-10
//          rule "an edit typed while Revert waits is kept" is retracted;
//          the full matrix is in audit-r11-ed.e2e.ts)
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
import type { Locator, Page } from "@playwright/test";
import fs from "node:fs";
import path from "node:path";
import { LIBRARY_DIR } from "./env";
import { createExample, expect, loadProject, noToast, test, waitForProject, type Locale } from "./fixtures";
import { LOCALES, cs, tr } from "./i18n";
import { hold, holdLock, olderOffer, timeline } from "./review";

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

/** Types `text` into the job's customer field and leaves it (the value is stored then). */
async function typeCustomer(page: Page, text: string) {
  await dockTab(page, "dock.job").click();
  await customer(page).click();
  await customer(page).press("ControlOrMeta+a");
  await page.keyboard.type(text);
  await page.keyboard.press("Tab");
}

test.describe("round 10, rewritten in round 11 (design A1): an edit tried while Revert or the put-back is with the core is refused, nothing is lost (ERR-901)", () => {
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
    test(`${action}: an edit tried while the call waits for the lock is refused, and the value typed after the answer is saved (ERR-901, design A1)`, async ({ app, page, mcp }) => {
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
      const saves = timeline(page, "save_project_checked");
      // Held until the intruder was tried, never for a fixed time (TEST-706, TEST-1004).
      const lock = await holdLock(id, 60_000);
      try {
        await press();
        await expect.poll(() => call.sent.length, { timeout: 4000 }).toBe(1);
        await expect(page.locator(".statusbar .save")).toHaveText(cs(action === "put-back" ? "editor.restoringOffer" : "editor.reverting"));
        // The job tab and its field take no press while the call is out (mouse at their place: they are inert).
        for (const l of [dockTab(page, "dock.job"), customer(page)]) {
          const box = (await l.count()) ? await l.first().boundingBox() : null;
          if (box) await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
        }
        await page.keyboard.type("Novák během");
        await page.keyboard.press("Tab");
        await page.waitForTimeout(600);
        expect(call.answered, "the call is still out").toHaveLength(0);
        expect(saves.sent, "nothing is saved while the call is out").toHaveLength(0);
      } finally {
        lock.kill();
      }
      const done = await waitForProject(id, (p) => (action === "put-back" ? p.result_snapshot?.engine_version === "0.8.0" : !p.notes), 12_000);
      if (action === "put-back") expect(done.result_snapshot.roof_area).toBe(150);
      await app.saved();
      expect((await loadProject(id)).job?.customer ?? "").not.toBe("Novák během");
      // Nothing lost: the editor takes the value now, and it lands within 3 s.
      await typeCustomer(page, "Novák potom");
      await waitForProject(id, (p) => p.job?.customer === "Novák potom", 3000);
      await app.saved();
      await expect(customer(page)).toHaveValue("Novák potom");
    });

    test(`${action}: a value typed before the press (no Enter) is saved before the call and kept by the answer (ERR-901, design A1)`, async ({ app, page, mcp }) => {
      let id: string;
      let press: () => Promise<void>;
      if (action === "put-back") {
        id = await putBackReady(app, page, mcp, `Před ${action}`);
        press = () => putBack(page).click();
      } else {
        const r = await revertReady(app, page, mcp, `Před ${action}`);
        id = r.id;
        press = () => r.revert.click();
      }
      await dockTab(page, "dock.job").click();
      await customer(page).click();
      await customer(page).press("ControlOrMeta+a");
      await page.keyboard.type("Novák předem");
      await dockTab(page, "dock.ai").click();
      await press();
      const done = await waitForProject(id, (p) => p.job?.customer === "Novák předem" && (action === "put-back" ? p.result_snapshot?.engine_version === "0.8.0" : !p.notes), 8000);
      if (action === "put-back") expect(done.result_snapshot.roof_area).toBe(150);
      await app.saved();
      await dockTab(page, "dock.job").click();
      await expect(customer(page)).toHaveValue("Novák předem");
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
    // Not in the library: never seen there (TEST-1002, a strict check).
    await noToast(recalcToast(app), 1000, "the notice in the library");
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
    await noToast(recalcToast(app), 1500, "a second notice");
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
