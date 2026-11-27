// Guards for the round-7 audit (2026-11-22) outside the drag class.
//
// UI-604: the dialogs an AI request opens (read a file, move a folder), the
// AI settings card and the AI panel's Revert kept `disabled` on the pressed
// button while they wait, so the keyboard focus dropped to the page (the
// editor's own dialogs were fixed in round 6, UI-503).
import fs from "node:fs";
import path from "node:path";
import type { Locator, Page, Route } from "@playwright/test";
import { E2E_DIR } from "./env";
import { createExample, expect, test } from "./fixtures";
import { cs } from "./i18n";

/** Holds every answer of the bridge to `cmd` for `ms`. */
async function slow(page: Page, cmd: string, ms: number) {
  await page.route(new RegExp(`/${cmd}$`), async (r: Route) => {
    await new Promise((res) => setTimeout(res, ms));
    await r.continue();
  });
}

/** Enter on the focused button: it says busy and keeps the focus while its action waits. */
async function staysFocusedWhileWaiting(page: Page, button: Locator) {
  await expect(button).toBeEnabled();
  await button.focus();
  await expect(button).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(button).toHaveAttribute("aria-busy", "true");
  // Read as it is, while the answer is held: not disabled, still focused.
  expect(await button.evaluate((el: HTMLButtonElement) => ({ disabled: el.disabled, focused: document.activeElement === el }))).toEqual({ disabled: false, focused: true });
}

test.describe("round-7 audit: a button that waits keeps the keyboard focus (UI-604)", () => {
  test("the file request's Allow", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "Soubor");
    await app.open();
    const outside = path.join(E2E_DIR, "elsewhere-r7", "plan.pdf");
    fs.mkdirSync(path.dirname(outside), { recursive: true });
    fs.writeFileSync(outside, "%PDF-1.4");
    expect((await (mcp as any).tryCall("attach_file", { id, path: outside, summary: "Attach the plan" })).isError).toBe(true);
    const dlg = page.getByRole("dialog", { name: cs("fileReq.title") });
    await expect(dlg).toBeVisible();
    await slow(page, "answer_file_request", 1200);
    await staysFocusedWhileWaiting(page, dlg.getByRole("button", { name: cs("folderReq.allow"), exact: true }));
    await expect(dlg).toHaveCount(0);
  });

  test("the folder request's Keep", async ({ app, page, mcp }) => {
    await app.open();
    const other = path.join(E2E_DIR, "ai-library-r7");
    fs.mkdirSync(other, { recursive: true });
    await mcp.call("update_settings", { projects_dir: other, summary: "Use the shared folder" });
    const dlg = page.getByRole("dialog", { name: cs("folderReq.title") });
    await expect(dlg).toBeVisible();
    await slow(page, "answer_folder_request", 1200);
    await staysFocusedWhileWaiting(page, dlg.getByRole("button", { name: cs("folderReq.keep") }));
    await expect(dlg).toHaveCount(0);
  });

  test("the AI settings card's Accept", async ({ app, page, mcp }) => {
    await app.open();
    await mcp.call("update_settings", { roof_defaults: { pitch_deg: 25 }, summary: "Company defaults" });
    const card = page.getByRole("status", { name: cs("aiChange.title", { by: "Claude" }) });
    await expect(card).toBeVisible();
    await slow(page, "answer_ai_change", 1200);
    await staysFocusedWhileWaiting(page, card.getByRole("button", { name: cs("review.accept") }));
    await expect(card).toHaveCount(0);
  });

  test("the AI panel's Revert", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "Vrátit");
    await app.open();
    await app.openJob("Vrátit");
    await mcp.call("update_job", { id, notes: "AI", summary: "AI note." });
    const panel = page.locator(".editor aside .dock");
    const revert = panel.getByRole("button", { name: cs("ai.revert") }).first();
    await expect(revert).toBeVisible();
    await slow(page, "revert_last_change", 1200);
    await staysFocusedWhileWaiting(page, revert);
    await expect(page.locator(".review")).toHaveCount(0, { timeout: 8000 });
  });
});
