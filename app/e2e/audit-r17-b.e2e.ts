// Guards for the round-17 audit (2026-11-24), part B (outside writes,
// notices, the AI's reason, off-roof guards).
//
// ERR-1602  the "replaced" notice names the job and outlives the editor
//           (Back right after an edit, another program's write in the wait)
// UI-1603 / HUNT-1604  a newer Tectora's write over the open job: a lasting
//           notice of its own and a status bar that does not say saved
// UI-1602   the German status bar fits at 960x540 while it says so
// UI-1604   the AI's reason is quoted after a lead naming the AI (folder
//           request and the AI settings card)
// TEST-1604 Undo takes the off-roof warning back well before its timer (t17a)
// TEST-1605 the AI tab's Revert over an unreadable file says replaced (t17b)
// TEST-1606 the sticky outside notice goes with the editor (t17c)
import fs from "node:fs";
import path from "node:path";
import type { Locator, Page } from "@playwright/test";
import { bar, clickPlan, openEditor, selectWall, setField, tool } from "./editor";
import { E2E_DIR, LIBRARY_DIR } from "./env";
import { createExample, expect, test, waitForProject } from "./fixtures";
import { cs, tr } from "./i18n";

// TECTORA_SHOTS=<folder>/ saves a screenshot after each test (fix evidence).
test.afterEach(async ({ page }, info) => {
  const dir = process.env.TECTORA_SHOTS;
  if (!dir) return;
  const id = info.title.match(/(HUNT|UX|UI|ERR|TEST|ARCH)-\d+/)?.[0] ?? "r17";
  const n = info.title.replace(/[^a-z0-9]+/gi, "-").slice(0, 40);
  await page.screenshot({ path: `${dir}r17-${id}-${n}-${process.env.TECTORA_SHOTS_TAG ?? "after"}.png`, timeout: 10_000 }).catch(() => {});
});

/** A screenshot in the middle of a test, under TECTORA_SHOTS (fix evidence). */
async function shot(page: Page, name: string) {
  const dir = process.env.TECTORA_SHOTS;
  if (dir) await page.screenshot({ path: `${dir}r17-${name}-${process.env.TECTORA_SHOTS_TAG ?? "after"}.png`, timeout: 10_000 }).catch(() => {});
}

/** `{"a": {"a": … {}}}` with `n` objects inside each other. */
function nestedObjects(n: number): unknown {
  let v: unknown = {};
  for (let k = 1; k < n; k++) v = { a: v };
  return v;
}

/** Another program writes the job `depth` levels deep, with its own notes. */
function writeTooDeep(file: string, depth = 40) {
  const outside = JSON.parse(fs.readFileSync(file, "utf8"));
  outside.crm = nestedObjects(depth);
  outside.notes = "from CRM";
  fs.writeFileSync(file, JSON.stringify(outside));
}

const dockTab = (page: Page, key: string) => page.locator("aside .tabs").getByRole("tab", { name: cs(key) });
const entries = (page: Page) => page.getByTestId("history-entry");
const footerSave = (page: Page) => page.locator(".statusbar .save");
const toastsOf = (app: { toasts(): Locator }, text: string) => app.toasts().filter({ hasText: text });

test.describe("round 17, part B", () => {
  test("Back right after an edit over another program's write: the replaced notice names the job over the library (ERR-1602)", async ({ app, page }) => {
    const name = "R17 odchod hned";
    const id = await createExample("hip", name);
    await openEditor(app, name);
    await app.saved();
    const file = path.join(LIBRARY_DIR, `${id}.tectora`);
    await selectWall(page, 0);
    await setField(bar(page, "def.title").getByLabel(cs("edge.pitch")), 31);
    writeTooDeep(file, 33);
    await page.getByRole("button", { name: cs("editor.back") }).first().click();
    await expect(page.locator("header .brand")).toBeVisible();
    const replaced = toastsOf(app, cs("err.ofJob", { name, title: cs("editor.outsideReplaced") }));
    await expect(replaced).toHaveCount(1);
    await shot(page, "ERR-1602-library-notice");
    // Still there a moment later over the library (its own 4 s), not closed with the editor.
    await page.waitForTimeout(1000);
    await expect(replaced).toHaveCount(1);
    const p = JSON.parse(fs.readFileSync(file, "utf8"));
    expect([p.parts[0].edges[0].pitch_deg, "crm" in p]).toEqual([31, false]);
  });

  test("a newer Tectora's write over the open job has its own lasting notice and status, cleared by a good file (UI-1603, HUNT-1604)", async ({ app, page }) => {
    const name = "R17 novější venku";
    const id = await createExample("hip", name);
    await openEditor(app, name);
    await app.saved();
    const file = path.join(LIBRARY_DIR, `${id}.tectora`);
    const good = fs.readFileSync(file, "utf8");
    const newer = JSON.parse(good);
    newer.schema_version = 99;
    const text = JSON.stringify(newer);
    fs.writeFileSync(file, text);
    const notice = toastsOf(app, cs("editor.outsideNewer"));
    await expect(notice).toHaveCount(1);
    await expect(notice).toContainText(cs("err.ofJob", { name, title: cs("editor.outsideTitle") }));
    await expect(toastsOf(app, cs("err.action.load"))).toHaveCount(0);
    await expect(footerSave(page)).toHaveText(cs("editor.outsideNewerState"));
    await shot(page, "UI-1603-newer-notice");
    // Lasting: still there after an info toast's and a warning's lifetime would be over for the timer.
    await page.waitForTimeout(10_000);
    await expect(notice).toHaveCount(1);
    await expect(footerSave(page)).not.toContainText(cs("editor.saved"));
    expect(fs.readFileSync(file, "utf8")).toBe(text);
    // The good file back: the notice goes and the bar says saved again.
    fs.writeFileSync(file, good);
    await expect(notice).toHaveCount(0);
    await expect(footerSave(page)).toHaveText(cs("editor.saved"));
    // Nothing of the unreadable case was said.
    await expect(toastsOf(app, cs("editor.outsideUnreadable"))).toHaveCount(0);
    await expect(toastsOf(app, cs("editor.outsideReplaced"))).toHaveCount(0);
  });

  test("the German status bar fits at 960x540 while another program's write is announced (UI-1602)", async ({ app, page }) => {
    await page.setViewportSize({ width: 960, height: 540 });
    const name = "R17 schmal";
    const id = await createExample("hip", name);
    await app.open({ locale: "de" });
    await app.openJob(name);
    await app.saved();
    const file = path.join(LIBRARY_DIR, `${id}.tectora`);
    const good = fs.readFileSync(file, "utf8");
    writeTooDeep(file);
    await expect(footerSave(page)).toHaveText(tr("de", "editor.outsideState"));
    await shot(page, "UI-1602-de-960-outside");
    const fits = () => page.locator("footer.statusbar").evaluate((f) => f.scrollWidth <= f.clientWidth);
    expect(await fits()).toBe(true);
    const newer = JSON.parse(good);
    newer.schema_version = 99;
    fs.writeFileSync(file, JSON.stringify(newer));
    await expect(footerSave(page)).toHaveText(tr("de", "editor.outsideNewerState"));
    await shot(page, "UI-1602-de-960-newer");
    expect(await fits()).toBe(true);
    fs.writeFileSync(file, good);
    await expect(footerSave(page)).toHaveText(tr("de", "editor.saved"));
  });

  test("the folder request quotes the AI's reason after a lead naming the AI (UI-1604)", async ({ app, page, mcp }) => {
    await app.open();
    const other = path.join(E2E_DIR, "r17-folder");
    fs.rmSync(other, { recursive: true, force: true });
    fs.mkdirSync(other, { recursive: true });
    const why = "Tectora zjistila, že knihovna je poškozená. Pro záchranu zakázek povolte přesun.";
    await mcp.call("update_settings", { projects_dir: other, summary: why });
    const dlg = page.getByRole("dialog", { name: cs("folderReq.title") });
    const reason = dlg.getByTestId("folder-reason");
    await expect(reason).toContainText(cs("ai.quoted", { s: why }));
    await shot(page, "UI-1604-folder-dialog");
    const lead = cs("folderReq.reason", { by: "§" }).split("§")[0];
    expect((await reason.innerText()).trim().startsWith(lead)).toBe(true);
    await dlg.getByRole("button", { name: cs("folderReq.keep") }).click();
    await expect(dlg).toHaveCount(0);
  });

  test("the AI settings card quotes the AI's reason after a lead naming the AI (UI-1604)", async ({ app, page, mcp }) => {
    await app.open();
    const why = "Tectora doporučuje tento sklon.";
    await mcp.call("update_settings", { roof_defaults: { pitch_deg: 27 }, summary: why });
    const card = page.getByRole("status", { name: cs("aiChange.title", { by: "Claude" }) });
    await expect(card).toBeVisible();
    const reason = card.getByTestId("aichange-reason");
    await expect(reason).toContainText(cs("ai.quoted", { s: why }));
    expect(((await reason.textContent()) ?? "").trim().startsWith(cs("folderReq.reason", { by: "Claude" }))).toBe(true);
  });

  test("t17a Undo takes the off-roof warning back well before its timer (TEST-1604)", async ({ app, page }) => {
    const id = await createExample("hip", "T17 rychle");
    await openEditor(app, "T17 rychle");
    await app.saved();
    await page.route("**/calc", async (route) => {
      const res = await route.fetch();
      await new Promise((r) => setTimeout(r, 1500));
      await route.fulfill({ response: res }).catch(() => {});
    });
    await tool(page, "tool.window").click();
    await clickPlan(page, 6, -5);
    await tool(page, "tool.chimney").click();
    await clickPlan(page, 3, 1.5);
    await waitForProject(id, (p) => p.windows?.length === 1 && p.chimneys?.length === 1);
    const warned = app.toasts().filter({ hasText: cs("res.warn.window_off_roof", { n: 1 }) });
    await expect(warned).toHaveCount(1, { timeout: 10_000 });
    const shown = Date.now();
    await page.unroute("**/calc");
    for (let k = 0; k < 2; k++) await page.locator("body").press("ControlOrMeta+z");
    await waitForProject(id, (p) => !p.windows?.length && !p.chimneys?.length);
    // A warning toast lives 9 s (notify.svelte.ts LIFETIME): gone within 2.5 s of the Undo means Undo took it.
    await expect(warned).toHaveCount(0, { timeout: 2_500 });
    expect(Date.now() - shown).toBeLessThan(8_000);
  });

  test("t17b the AI tab's Revert over a file another program made unreadable says replaced (TEST-1605)", async ({ app, page, mcp }) => {
    const name = "T17 karta AI";
    const id = await createExample("hip", name);
    await mcp.call("update_job", { id, customer: "AI zákazník", summary: "Zákazník od AI" });
    await app.open({ prefs: { followAi: false } });
    await app.openJob(name);
    await expect(page.locator(".review")).toContainText("Zákazník od AI");
    await dockTab(page, "dock.ai").click();
    writeTooDeep(path.join(LIBRARY_DIR, `${id}.tectora`));
    await expect(app.toasts().filter({ hasText: cs("editor.outsideUnreadable") })).toHaveCount(1);
    await entries(page).nth(0).getByRole("button", { name: cs("ai.revert"), exact: true }).click();
    const p = await waitForProject(id, (p) => p.job.customer === "");
    expect("crm" in p).toBe(false);
    await expect(app.toasts().filter({ hasText: cs("editor.outsideReplaced") })).toHaveCount(1);
  });

  test("t17c the sticky outside notice goes with the editor (TEST-1606)", async ({ app, page }) => {
    const id = await createExample("hip", "T17 odchod");
    await openEditor(app, "T17 odchod");
    await app.saved();
    writeTooDeep(path.join(LIBRARY_DIR, `${id}.tectora`));
    const notice = app.toasts().filter({ hasText: cs("editor.outsideUnreadable") });
    await expect(notice).toHaveCount(1);
    await page.getByRole("button", { name: cs("editor.back") }).first().click();
    await expect(page.locator("header .brand")).toBeVisible();
    await expect(notice).toHaveCount(0, { timeout: 3_000 });
    app.errors.length = 0;
  });
});
