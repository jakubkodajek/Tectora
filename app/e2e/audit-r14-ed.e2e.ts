// Guards for the round-14 audit (2026-11-23), the editor's half.
//
// Design A: the Job tab's "keeps the result stored earlier" toast belongs to
// its editor (HUNT-1302, TEST-1304): it stays through an unrelated edit,
// closes with the editor, and its button still stores from another dock tab.
// Design D: a job whose id is an Object.prototype name ("constructor") can be
// duplicated and deleted (ARCH-1301).
// Design E: the prepare phase names what held it (UX-1301) and the dialog
// counter does not leak (TEST-1301).
// UI-1301: a refused Revert all is titled in the plural.
// Design F: the AI's folder request says what Allow does (UX-1302).
// ARCH-1303: the library's broken-file list explains a too-deep job.
import type { Page } from "@playwright/test";
import fs from "node:fs";
import path from "node:path";
import { selectWall } from "./editor";
import { E2E_DIR, LIBRARY_DIR } from "./env";
import { bridge, createExample, expect, listProjects, loadProject, test, waitForProject } from "./fixtures";
import { cs } from "./i18n";
import { acceptAll, hold } from "./review";

// TECTORA_SHOTS=<folder>/ saves a screenshot after each test (fix evidence).
test.afterEach(async ({ page }, info) => {
  const dir = process.env.TECTORA_SHOTS;
  if (!dir) return;
  const id = info.title.match(/(HUNT|UX|UI|ERR|TEST|ARCH)-\d+/)?.[0] ?? "r14";
  const n = info.title.replace(/[^a-z0-9]+/gi, "-").slice(0, 40);
  await page.screenshot({ path: `${dir}r14-${id}-${n}-${process.env.TECTORA_SHOTS_TAG ?? "after"}.png`, timeout: 10_000 }).catch(() => {});
});

const dockTab = (page: Page, key: string) => page.locator("aside .tabs").getByRole("tab", { name: cs(key) });
const statusSelect = (page: Page) => page.locator("aside .job select").first();
const back = (page: Page) => page.getByRole("button", { name: cs("editor.back") }).first();

test.describe("round 14: the kept-result toast belongs to its editor (design A, HUNT-1302)", () => {
  /**
   * A hip job whose stored result is older than its roof, open in the
   * editor; the Job tab's status goes to offer and the "keeps the result
   * stored earlier" toast is up. Returns the stored result it quotes.
   */
  async function keptToast(app: any, page: Page, mcp: any, name: string) {
    const id = await createExample("hip", name);
    await mcp.call("update_job", { id, status: "offer", summary: "Nabídka" });
    const before = (await waitForProject(id, (p) => !!p.result_snapshot)).result_snapshot;
    await mcp.call("update_job", { id, status: "draft", summary: "Zpět na návrh" });
    await mcp.call("update_edges", { id, changes: [{ index: 0, pitch_deg: 40 }], summary: "Sklon podle řezu" });
    await acceptAll(id);
    await app.open({ prefs: { followAi: false } });
    await app.openJob(name);
    await app.saved();
    await dockTab(page, "dock.job").click();
    await statusSelect(page).selectOption("offer");
    const toast = app.toasts().filter({ hasText: cs("job.offerKeptTitle") });
    await expect(toast).toBeVisible({ timeout: 10000 });
    await waitForProject(id, (p) => p.job.status === "offer");
    await app.saved();
    return { id, before, toast };
  }

  test("the toast names its job and stays through an edit that leaves the stored result alone (TEST-1304 a)", async ({ app, page, mcp }) => {
    const { id, before, toast } = await keptToast(app, page, mcp, "R14 nechat");
    await expect(toast).toContainText(cs("err.ofJob", { name: "R14 nechat", title: cs("job.offerKeptTitle") }));
    const number = page.locator("aside .job").getByLabel(cs("job.number"));
    await number.fill("N-14");
    await number.press("Enter");
    await waitForProject(id, (p) => p.job.number === "N-14");
    await app.saved();
    await expect(toast, "an unrelated edit does not close it").toHaveCount(1);
    expect((await loadProject(id)).result_snapshot).toEqual(before);
  });

  test("Back closes the toast, so nothing can store into the job that was left (HUNT-1302, TEST-1304 b)", async ({ app, page, mcp }) => {
    const { id, before, toast } = await keptToast(app, page, mcp, "R14 zpět");
    // Hovered, its 9 s timer starts again on leaving: it would still be up after Back.
    await toast.hover();
    await back(page).click();
    await expect(page.locator(".editor")).toHaveCount(0);
    expect(await toast.count(), "closed with its editor, not by its timer").toBe(0);
    await expect(page.getByRole("button", { name: cs("snap.store") })).toHaveCount(0);
    await page.waitForTimeout(500);
    const p = await loadProject(id);
    expect(p.result_snapshot, "the offer's figures are untouched").toEqual(before);
  });

  test("another job opened at once shows no toast of the job left, and the left job's figures stay (HUNT-1302, TEST-1304 b)", async ({ app, page, mcp }) => {
    const other = await createExample("gable", "R14 jiná");
    const { id, before, toast } = await keptToast(app, page, mcp, "R14 první");
    await toast.hover();
    await back(page).click();
    await app.openJob("R14 jiná");
    expect(await toast.count(), "closed with its editor, not by its timer").toBe(0);
    await app.saved();
    await expect(page.getByRole("button", { name: cs("snap.store") })).toHaveCount(0);
    await page.waitForTimeout(500);
    expect((await loadProject(id)).result_snapshot, "the left job's offer figures are untouched").toEqual(before);
    expect((await loadProject(other)).result_snapshot ?? null).toBeNull();
  });

  test("the toast's button still stores the result after the dock moved to Results (TEST-1304 c)", async ({ app, page, mcp }) => {
    const { id, before, toast } = await keptToast(app, page, mcp, "R14 výsledky");
    await dockTab(page, "dock.results").click();
    await toast.getByRole("button", { name: cs("snap.store") }).click();
    const p = await waitForProject(id, (p) => Math.abs(p.result_snapshot.roof_area - before.roof_area) > 0.01, 8000);
    expect(p.job.status).toBe("offer");
    await app.saved();
  });
});

test.describe("round 14: job ids that are Object.prototype names (design D, ARCH-1301)", () => {
  test("a job named Constructor can be duplicated and deleted in the library (ARCH-1301)", async ({ app, page }) => {
    const id = await createExample("gable", "Constructor");
    expect(id).toBe("constructor");
    await app.open();
    await page.getByRole("row").filter({ hasText: "Constructor" }).first().click();
    const detail = page.locator("aside.detail");
    await expect(detail.getByRole("heading", { name: "Constructor" })).toBeVisible();
    const duplicate = detail.getByRole("button", { name: cs("card.duplicate") });
    const remove = detail.getByRole("button", { name: cs("card.delete") });
    await expect(duplicate).toBeEnabled();
    await expect(remove).toBeEnabled();
    const before = (await listProjects()).length;
    await duplicate.click();
    await expect.poll(async () => (await listProjects()).length).toBe(before + 1);
    await page.getByRole("row").filter({ hasText: "Constructor" }).filter({ hasNotText: cs("lib.copyName", { name: "Constructor" }) }).first().click();
    await expect(detail.getByRole("heading", { name: "Constructor", exact: true })).toBeVisible();
    await expect(remove).toBeEnabled();
    await app.answerConfirm(true);
    await remove.click();
    await expect.poll(async () => (await listProjects()).some((p: any) => p.id === "constructor")).toBe(false);
  });
});

const bannerRevert = (page: Page) => page.locator(".review").getByRole("button", { name: cs("ai.revert") });

/** A gable job open in the editor with the AI's notes pending (and, with `two`, its customer too). */
async function readyAi(app: any, page: Page, mcp: any, name: string, two = false, chimney = false) {
  const id = await createExample("gable", name);
  if (chimney) {
    const p0 = await loadProject(id);
    p0.chimneys = [{ corners: [{ x: 5, y: 1.5 }, { x: 5.6, y: 1.5 }, { x: 5.6, y: 2.1 }, { x: 5, y: 2.1 }], height: 8 }];
    await bridge("save_project", { id, project: p0 });
    await acceptAll(id);
  }
  await app.open({ prefs: { followAi: false } });
  await app.openJob(name);
  await app.saved();
  await mcp.call("update_job", { id, notes: "Od AI", summary: "Poznámka od AI" });
  await waitForProject(id, (p) => p.notes === "Od AI");
  if (two) {
    await mcp.call("update_job", { id, customer: "Zákazník od AI", summary: "Zákazník podle výkresu" });
    await waitForProject(id, (p) => p.job.customer === "Zákazník od AI");
  }
  await app.saved();
  await expect(page.locator(".review")).toBeVisible();
  return id;
}

test.describe("round 14: the prepare phase and its dialogs (design E, TEST-1301)", () => {
  test("after-dialog: Settings and the object dialog opened and closed before the press do not hold Revert (TEST-1301)", async ({ app, page, mcp }) => {
    const id = await readyAi(app, page, mcp, "R14 po dialogu", false, true);
    const settings = page.getByRole("dialog", { name: cs("settings.title") });
    await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
    await page.keyboard.press("ControlOrMeta+Comma");
    await expect(settings).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(settings).toHaveCount(0);
    const bb = (await page.locator("path.opening.chimney").first().boundingBox())!;
    await page.mouse.dblclick(bb.x + bb.width / 2, bb.y + bb.height / 2);
    const dlg = page.getByTestId("object-dialog");
    await expect(dlg).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(dlg).toHaveCount(0);
    await app.saved();
    await bannerRevert(page).click();
    await waitForProject(id, (p) => !p.notes, 8000);
    await app.saved();
    await expect(app.toasts().filter({ hasText: cs("err.busy_edits") })).toHaveCount(0);
    await expect(app.toasts().filter({ hasText: cs("err.busy_open") })).toHaveCount(0);
  });
});

test.describe("round 14: Revert all refused is titled in the plural (UI-1301)", () => {
  test("a busy Revert all of two AI steps says the changes could not be reverted (UI-1301)", async ({ app, page, mcp }) => {
    const id = await readyAi(app, page, mcp, "R14 vše", true);
    await expect(page.locator(".review").getByRole("button", { name: cs("review.revertAll") })).toBeVisible();
    let open!: () => void;
    const gate = new Promise<void>((r) => (open = r));
    let n = 0;
    await page.route("**/revert_changes", async (route) => {
      n++;
      await gate;
      await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ err: { code: "busy", id, detail: "the job is locked by another writer" } }) }).catch(() => {});
    });
    await app.answerConfirm(true);
    await page.locator(".review").getByRole("button", { name: cs("review.revertAll") }).click();
    await expect.poll(() => n, { timeout: 5000 }).toBe(1);
    open();
    await expect(app.toasts().filter({ hasText: cs("err.ofJob", { name: "R14 vše", title: cs("err.action.revertAll") }) })).toHaveCount(1);
    await expect(app.toasts().filter({ hasText: cs("err.action.revert") })).toHaveCount(0);
    app.errors.length = 0;
  });
});

test.describe("round 14: the library's broken-file list (ARCH-1303)", () => {
  test("a job with unknown keys nested too deep is explained as such, not as the operating system's refusal (ARCH-1303)", async ({ app, page }) => {
    const id = await createExample("gable", "R14 hloubka");
    const file = path.join(LIBRARY_DIR, `${id}.tectora`);
    const p = JSON.parse(fs.readFileSync(file, "utf8"));
    let v: unknown = {};
    for (let i = 0; i < 40; i++) v = { a: v };
    p.crm = v;
    fs.writeFileSync(file, JSON.stringify(p));
    await app.open();
    const broken = page.locator(".broken").filter({ hasText: cs("lib.broken", { n: 1 }) });
    await expect(broken).toBeVisible();
    await broken.getByRole("button", { name: cs("lib.brokenShow") }).click();
    const line = broken.locator("li").filter({ hasText: `${id}.tectora` });
    await expect(line).toContainText(cs("err.unknown_keys"));
    await expect(line).not.toContainText(cs("err.io"));
  });
});

test.describe("round 14: the AI's folder request says what Allow does (design F, UX-1302)", () => {
  async function askFolder(mcp: any, name: string) {
    const other = path.join(E2E_DIR, `r14-${name}`);
    fs.rmSync(other, { recursive: true, force: true });
    fs.mkdirSync(other, { recursive: true });
    await mcp.call("update_settings", { projects_dir: other, summary: "Use the shared folder" });
  }
  const request = (page: Page) => page.getByRole("dialog", { name: cs("folderReq.title") });

  test("with a job open the request says the job is saved and closed and the jobs stay in the old folder (UX-1302)", async ({ app, page, mcp }) => {
    await createExample("gable", "R14 složka");
    await app.open();
    await app.openJob("R14 složka");
    await app.saved();
    await askFolder(mcp, "open");
    await expect(request(page)).toContainText(cs("folderReq.jobCloses"));
    await expect(request(page)).toContainText(cs("folderReq.jobsStay"));
  });

  test("in the library the request says only that the jobs stay in the old folder (UX-1302)", async ({ app, page, mcp }) => {
    await createExample("gable", "R14 knihovna");
    await app.open();
    await askFolder(mcp, "library");
    await expect(request(page)).toContainText(cs("folderReq.jobsStay"));
    await expect(request(page)).not.toContainText(cs("folderReq.jobCloses"));
  });
});
