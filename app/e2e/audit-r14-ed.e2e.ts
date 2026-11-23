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
import { createExample, expect, loadProject, test, waitForProject } from "./fixtures";
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
