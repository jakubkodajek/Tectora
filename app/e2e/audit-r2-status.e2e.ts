// Guards for the round-2 audit (2026-11-21) job status findings: a status
// change survives an AI save during the result's calculation (ERR-102), and
// an empty job cannot be offered (UX-102).
import type { Page } from "@playwright/test";
import { openEditor } from "./editor";
import { createExample, expect, loadProject, test, waitForProject } from "./fixtures";
import { cs } from "./i18n";

const jobTab = (page: Page) => page.locator("aside .tabs").getByRole("tab", { name: cs("dock.job") });

test.describe("round-2 audit: job status", () => {
  test("a status change survives an AI save while the result is being stored (ERR-102)", async ({ app, page, mcp }) => {
    const id = await createExample("hip", "Nabídka a AI");
    await openEditor(app, "Nabídka a AI");
    await page.route(/\/take_snapshot$/, async (r) => {
      await new Promise((res) => setTimeout(res, 2500));
      await r.continue();
    });
    await jobTab(page).click();
    const status = page.locator("aside .job select");
    await status.selectOption("offer");
    await page.waitForTimeout(300);
    await mcp.call("update_job", { id, notes: "AI note", summary: "AI adds a note." });
    const p = await waitForProject(id, (p) => p.job.status === "offer" && !!p.result_snapshot, 12000);
    expect(p.notes).toBe("AI note");
    await jobTab(page).click();
    await expect(page.locator("aside .job select")).toHaveValue("offer");
  });

  test("a status change is refused with a note when an AI save changed the roof meanwhile (ERR-102)", async ({ app, page, mcp }) => {
    const id = await createExample("hip", "Nabídka a sklon");
    await openEditor(app, "Nabídka a sklon");
    await page.route(/\/take_snapshot$/, async (r) => {
      await new Promise((res) => setTimeout(res, 2500));
      await r.continue();
    });
    await jobTab(page).click();
    await page.locator("aside .job select").selectOption("offer");
    await page.waitForTimeout(300);
    await mcp.call("update_edges", { id, changes: [{ index: 0, pitch_deg: 30 }], summary: "Wall 1 at 30°." });
    await expect(app.toasts().filter({ hasText: cs("job.statusStale") })).toBeVisible({ timeout: 12000 });
    const p = await loadProject(id);
    expect(p.job.status).toBe("draft");
    expect(p.result_snapshot ?? null).toBeNull();
    await jobTab(page).click();
    await expect(page.locator("aside .job select")).toHaveValue("draft");
  });

  test("an empty job cannot be set to Offer, Ordered or Done (UX-102)", async ({ app, page }) => {
    await app.open();
    await page.locator("header").getByRole("button", { name: cs("lib.new") }).click();
    const dlg = page.getByRole("dialog", { name: cs("new.title") });
    await dlg.getByLabel(cs("job.name")).fill("Prázdná nabídka");
    await dlg.getByRole("tab", { name: cs("new.draw") }).click();
    await dlg.getByRole("button", { name: cs("new.create") }).click();
    await page.keyboard.press("Escape");
    await jobTab(page).click();
    const status = page.locator("aside .job select");
    for (const s of ["offer", "ordered", "done"]) await expect(status.locator(`option[value="${s}"]`)).toHaveAttribute("disabled", "");
    await expect(status).toHaveAttribute("title", cs("res.empty"));
  });
});
