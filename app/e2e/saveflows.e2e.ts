// Saving, AI review and follow mode when things overlap: a revert after the
// user's own edits (ERR-001), an outside save after a failed save (ERR-002),
// an export after a failed save (ERR-003), and follow mode while the user is
// in the middle of something (UX-001).
import fs from "node:fs";
import path from "node:path";
import type { Page } from "@playwright/test";
import { bar, clickPlan, openEditor, selectWall, setField } from "./editor";
import { createExample, expect, listProjects, loadProject, scratchFile, test, waitForProject } from "./fixtures";
import { cs } from "./i18n";

const review = (page: Page) => page.locator(".review");
const failSaves = (page: Page, code = "permission_denied") =>
  page.route("**/save_project", (r) =>
    r.fulfill({ contentType: "application/json", headers: { "access-control-allow-origin": "*" }, body: JSON.stringify({ err: { code } }) }),
  );

test.describe("revert keeps the user's later edits (ERR-001)", () => {
  test("Revert in the banner undoes the AI step and keeps the wall the user fixed afterwards", async ({ app, page, mcp }) => {
    const id = await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    await mcp.call("update_edges", { id, changes: [{ index: 3, pitch_deg: 44 }], summary: "Strmější stěna 4" });
    await expect(review(page)).toBeVisible();
    await selectWall(page, 0);
    await setField(bar(page, "def.title").getByLabel(cs("edge.overhang")), 0.9);
    await waitForProject(id, (p) => p.parts[0].edges[0].overhang === 0.9);
    await review(page).getByRole("button", { name: cs("ai.revert") }).click();
    await expect(review(page)).toHaveCount(0);
    const p = await waitForProject(id, (p) => p.parts[0].edges[3].pitch_deg === 35);
    expect(p.parts[0].edges[0].overhang).toBe(0.9);
    // The editor shows the reverted job with the fix.
    await selectWall(page, 0);
    await expect(bar(page, "def.title").getByLabel(cs("edge.overhang"))).toHaveValue("0,9");
    await selectWall(page, 3);
    await expect(bar(page, "def.title").getByLabel(cs("edge.pitch"))).toHaveValue("35");
  });

  test("an edit not saved yet when Revert is pressed is kept too", async ({ app, page, mcp }) => {
    const id = await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    await mcp.call("update_edges", { id, changes: [{ index: 3, pitch_deg: 44 }], summary: "Strmější stěna 4" });
    await expect(review(page)).toBeVisible();
    await selectWall(page, 0);
    await setField(bar(page, "def.title").getByLabel(cs("edge.overhang")), 0.8);
    // Straight away, before the autosave timer.
    await review(page).getByRole("button", { name: cs("ai.revert") }).click();
    const p = await waitForProject(id, (p) => p.parts[0].edges[3].pitch_deg === 35);
    expect(p.parts[0].edges[0].overhang).toBe(0.8);
  });

  test("when the user changed the value the AI set, Revert refuses and says why", async ({ app, page, mcp }) => {
    const id = await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    await mcp.call("update_edges", { id, changes: [{ index: 3, pitch_deg: 44 }], summary: "Strmější stěna 4" });
    await expect(review(page)).toBeVisible();
    await selectWall(page, 3);
    await setField(bar(page, "def.title").getByLabel(cs("edge.pitch")), 40);
    await waitForProject(id, (p) => p.parts[0].edges[3].pitch_deg === 40);
    await review(page).getByRole("button", { name: cs("ai.revert") }).click();
    await expect(app.toasts().filter({ hasText: cs("err.revert_conflict") })).toBeVisible();
    await page.waitForTimeout(800);
    expect((await loadProject(id)).parts[0].edges[3].pitch_deg).toBe(40);
    await expect(review(page)).toBeVisible();
    app.errors.length = 0;
  });
});

test.describe("failed saves", () => {
  test("an outside save after a failed save keeps the unsaved edit, and Retry saves both (ERR-002)", async ({ app, page, mcp }) => {
    const id = await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    await failSaves(page);
    await selectWall(page, 0);
    const overhang = bar(page, "def.title").getByLabel(cs("edge.overhang"));
    await setField(overhang, 0.9);
    await expect(page.locator(".statusbar .save-failed")).toBeVisible({ timeout: 8000 });
    await mcp.call("update_edges", { id, changes: [{ index: 2, pitch_deg: 20 }], summary: "Sklon stěny 3" });
    await expect(review(page)).toBeVisible();
    await expect(overhang).toHaveValue("0,9");
    // Still not saved: the state must not claim otherwise.
    await expect(page.locator(".statusbar .save-failed")).toBeVisible();
    await page.unroute("**/save_project");
    await page.locator(".statusbar .retry").click();
    await app.saved();
    const p = await waitForProject(id, (p) => p.parts[0].edges[0].overhang === 0.9);
    expect(p.parts[0].edges[2].pitch_deg).toBe(20);
    app.errors.length = 0;
  });

  test("Export job after a failed save stops and says the job was not saved (ERR-003)", async ({ app, page }) => {
    await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    await failSaves(page, "io");
    await selectWall(page, 0);
    await setField(bar(page, "def.title").getByLabel(cs("edge.overhang")), 0.9);
    await expect(page.locator(".statusbar .save-failed")).toBeVisible({ timeout: 8000 });
    await page.locator("aside .tabs").getByRole("tab", { name: cs("dock.job") }).click();
    const out = path.join(path.dirname(scratchFile("x", "")), "stale.tectorajob");
    await app.answerFileDialog(out);
    await page.locator("aside .job").getByRole("button", { name: cs("files.export") }).click();
    await expect(app.toasts().filter({ hasText: cs("err.not_saved") })).toBeVisible();
    expect(fs.existsSync(out)).toBe(false);
    app.errors.length = 0;
  });
});

test.describe("follow mode waits for the user (UX-001)", () => {
  test("an AI reading another job does not switch to it", async ({ app, page, mcp }) => {
    await createExample("hip", "Moje");
    const other = await createExample("gable", "Jiná");
    await openEditor(app, "Moje");
    await mcp.call("get_project", { id: other });
    await mcp.call("calc", { id: other });
    // The status reached the app (header pill), and the job stays.
    await expect(page.locator("header .ai-pill")).toContainText("Jiná");
    await page.waitForTimeout(800);
    await expect(page.locator(".editor input.title")).toHaveValue("Moje");
  });

  test("an AI save to another job while the user draws offers Open instead of switching", async ({ app, page, mcp }) => {
    const other = await createExample("gable", "Jiná");
    await app.open();
    await page.locator("header").getByRole("button", { name: cs("lib.new") }).click();
    const dlg = page.getByRole("dialog", { name: cs("new.title") });
    await dlg.getByLabel(cs("job.name")).fill("Kreslená");
    await dlg.getByRole("tab", { name: cs("new.draw") }).click();
    await dlg.getByRole("button", { name: cs("new.create") }).click();
    const draw = page.getByRole("toolbar", { name: cs("draw.title") });
    await clickPlan(page, 2, 2);
    await clickPlan(page, 12, 2);
    await expect(draw).toContainText(cs("draw.points", { n: 2 }));
    await mcp.call("update_job", { id: other, customer: "Z výkresu", summary: "Zákazník" });
    const offer = app.toasts().filter({ hasText: cs("ai.followOffer", { who: "Claude" }) });
    await expect(offer).toBeVisible();
    await expect(page.locator(".editor input.title")).toHaveValue("Kreslená");
    await expect(draw).toContainText(cs("draw.points", { n: 2 }));
    await offer.getByRole("button", { name: cs("ai.followOpen") }).click();
    await expect(page.locator(".editor input.title")).toHaveValue("Jiná");
  });

  test("an AI save while the New job dialog is open keeps the dialog", async ({ app, page, mcp }) => {
    const other = await createExample("gable", "Jiná");
    await app.open();
    await page.locator("header").getByRole("button", { name: cs("lib.new") }).click();
    const dlg = page.getByRole("dialog", { name: cs("new.title") });
    await dlg.getByLabel(cs("job.name")).fill("Napůl vyplněná");
    await mcp.call("update_job", { id: other, customer: "Z výkresu", summary: "Zákazník" });
    await expect(app.toasts().filter({ hasText: cs("ai.followOffer", { who: "Claude" }) })).toBeVisible();
    await expect(dlg).toBeVisible();
    await expect(dlg.getByLabel(cs("job.name"))).toHaveValue("Napůl vyplněná");
    await expect(page.locator(".editor")).toHaveCount(0);
    expect(await listProjects()).toHaveLength(1);
  });
});
