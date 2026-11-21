// Guards for the round-3 audit (2026-11-21) UI and UX findings: the job
// status and the stored offer (ERR-201, UX-206, UI-206), the stored-offer
// comparison (UX-205), the AI settings card next to toasts and the folder
// request (UX-201, UI-210), compact problem banners (UI-205), the AI status
// row (UX-207) and the plan after an undone draft wall (UX-203).
import type { Page } from "@playwright/test";
import { bar, clickPlan, openEditor, plan, tool } from "./editor";
import { createExample, expect, loadProject, test, waitForProject } from "./fixtures";
import { cs } from "./i18n";
import { FILES_DIR } from "./env";
import fs from "node:fs";
import path from "node:path";

const jobTab = (page: Page) => page.locator("aside .tabs").getByRole("tab", { name: cs("dock.job") });
const resultsTab = (page: Page) => page.locator("aside .tabs").getByRole("tab", { name: cs("dock.results") });
const statusSelect = (page: Page) => page.locator("aside .job select");
/** Accepts the AI change the app shows for review. */
async function accept(page: Page) {
  const btn = page.locator(".review button.accept");
  await expect(btn).toBeVisible({ timeout: 10000 });
  await btn.click();
  await expect(page.locator(".review")).toHaveCount(0);
}

test.describe("round-3 audit: job status and the stored offer", () => {
  test("a status change is refused when the user changed the roof while its result was taken (ERR-201)", async ({ app, page }) => {
    const id = await createExample("hip", "Nabídka a plochá");
    await openEditor(app, "Nabídka a plochá");
    await page.route(/\/take_snapshot$/, async (r) => {
      await new Promise((res) => setTimeout(res, 2500));
      await r.continue();
    });
    await jobTab(page).click();
    await statusSelect(page).selectOption("offer");
    await page.waitForTimeout(300);
    // The user's own edit, in place on the project the panel holds.
    await tool(page, "tool.flat").click();
    await expect(app.toasts().filter({ hasText: cs("job.statusStale") })).toBeVisible({ timeout: 12000 });
    await app.saved();
    const p = await loadProject(id);
    expect(p.flat_roof).toBe(true);
    expect(p.job.status).toBe("draft");
    expect(p.result_snapshot ?? null).toBeNull();
    await jobTab(page).click();
    await expect(statusSelect(page)).toHaveValue("draft");
  });

  test("offering again after a change says the stored figures differ and can store the new ones (UX-206)", async ({ app, page, mcp }) => {
    const id = await createExample("hip", "Nová nabídka");
    await openEditor(app, "Nová nabídka");
    await mcp.call("update_job", { id, status: "offer", summary: "Offer sent." });
    const stored = (await waitForProject(id, (p) => !!p.result_snapshot)).result_snapshot.roof_area as number;
    await accept(page);
    await jobTab(page).click();
    await statusSelect(page).selectOption("draft");
    await waitForProject(id, (p) => p.job.status === "draft");
    await tool(page, "tool.flat").click();
    await jobTab(page).click();
    await statusSelect(page).selectOption("offer");
    const toast = app.toasts().filter({ hasText: cs("job.offerKeptTitle") });
    await expect(toast).toBeVisible({ timeout: 10000 });
    await expect(toast).toContainText(new Intl.NumberFormat("cs-CZ", { minimumFractionDigits: 2, maximumFractionDigits: 2 }).format(stored));
    await toast.getByRole("button", { name: cs("snap.store") }).click();
    const p = await waitForProject(id, (p) => p.job.status === "offer" && Math.abs(p.result_snapshot.roof_area - stored) > 0.01);
    expect(p.flat_roof).toBe(true);
  });

  test("offering again with an unchanged roof says nothing (UX-206)", async ({ app, page, mcp }) => {
    const id = await createExample("hip", "Stejná nabídka");
    await openEditor(app, "Stejná nabídka");
    await mcp.call("update_job", { id, status: "offer", summary: "Offer sent." });
    await accept(page);
    await jobTab(page).click();
    await statusSelect(page).selectOption("draft");
    await waitForProject(id, (p) => p.job.status === "draft");
    await statusSelect(page).selectOption("offer");
    await waitForProject(id, (p) => p.job.status === "offer");
    await page.waitForTimeout(500);
    await expect(app.toasts().filter({ hasText: cs("job.offerKeptTitle") })).toHaveCount(0);
  });

  test("an emptied job cannot store its result, and the comparison names its columns and lines (UI-206, UX-205)", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "Prázdná po nabídce");
    await openEditor(app, "Prázdná po nabídce");
    await mcp.call("update_job", { id, status: "offer", summary: "Offer sent." });
    await accept(page);
    // A change first: the comparison shows its columns and the line names.
    await tool(page, "tool.flat").click();
    await resultsTab(page).click();
    const diff = page.locator(".results table.diff");
    await expect(diff.locator("thead")).toContainText(cs("snap.colStored"));
    await expect(diff.locator("thead")).toContainText(cs("snap.colNow"));
    await tool(page, "tool.flat").click();
    // Now a verge change: the line code comes with its name.
    await mcp.call("update_edges", { id, changes: [{ index: 0, pitch_deg: 30 }], summary: "Wall 1 at 30°." });
    await accept(page);
    await resultsTab(page).click();
    await expect(diff).toContainText(`OL ${cs("bd.code.OL")}`, { timeout: 10000 });
    // Clear everything: no building, nothing to store.
    await app.answerConfirm(true);
    await tool(page, "tool.clear").click();
    await resultsTab(page).click();
    const store = page.locator(".results").getByRole("button", { name: cs("snap.store") });
    await expect(store).toBeDisabled();
    await expect(store).toHaveAttribute("title", cs("res.empty"));
  });
});

test.describe("round-3 audit: AI settings card", () => {
  test("at 600x420 the kept note does not cover the next card's answers (UX-201)", async ({ app, page, mcp }) => {
    await page.setViewportSize({ width: 600, height: 420 });
    await app.open({ locale: "de" });
    await mcp.call("update_settings", { roof_defaults: { pitch_deg: 25 }, summary: "A." });
    await mcp.call("update_settings", { roof_defaults: { pitch_deg: 35, overhang: 0.7 }, summary: "B." });
    const { bridge } = await import("./fixtures");
    await bridge("update_settings", { patch: { roof_defaults: { pitch_deg: 30 } } });
    const card = page.locator("section.card[role=status]");
    await expect(card).toBeVisible();
    await card.getByRole("button", { name: /Verwerfen|Vrátit/ }).first().click();
    await expect(app.toasts().filter({ hasText: "30°" })).toBeVisible();
    await expect(card).toBeVisible();
    for (const b of await card.getByRole("button").all()) {
      const box = (await b.boundingBox())!;
      const hit = await page.evaluate(
        ([x, y]) => !!document.elementFromPoint(x, y)?.closest("section.card"),
        [box.x + box.width / 2, box.y + box.height / 2],
      );
      expect(hit, `card button ${await b.textContent()} is clickable`).toBe(true);
    }
  });

  test("the AI settings card waits while the folder request is open (UI-210)", async ({ app, page, mcp }) => {
    await app.open();
    const dir = path.join(FILES_DIR, "drawings-r3");
    fs.mkdirSync(dir, { recursive: true });
    await mcp.call("update_settings", { drawings_dir: dir, roof_defaults: { pitch_deg: 37 }, summary: "Drawings elsewhere and pitch 37." });
    const dlg = page.getByRole("dialog");
    await expect(dlg).toBeVisible();
    await expect(page.locator("section.card[role=status]")).toHaveCount(0);
    await dlg.getByRole("button", { name: cs("folderReq.keep") }).click();
    await expect(page.locator("section.card[role=status]")).toBeVisible();
  });
});

test.describe("round-3 audit: editor banners and plan", () => {
  test("at 600x420 a long input problem keeps the plan usable (UI-205)", async ({ app, page, mcp }) => {
    await page.setViewportSize({ width: 600, height: 420 });
    const out = await mcp.call("create_project", {
      name: "Dvakrát",
      parts: [{ points: [[0, 0], [12, 0], [12, 8], [0, 8], [0, 0], [12, 0], [12, 8], [0, 8]], defaults: { pitch_deg: 35, eave_height: 3, overhang: 0.5 } }],
      summary: "Footprint.",
    });
    void out;
    // The app follows the AI into the new job.
    await app.open({ locale: "de" });
    const banner = page.locator(".warn-bar").filter({ hasText: "walls_overlap" });
    await expect(banner).toBeVisible();
    // 128 px before (German, the text in full on four lines).
    expect((await banner.boundingBox())!.height).toBeLessThanOrEqual(72);
    // The whole text is still there for the pointer.
    await expect(banner.locator(".warn-text span")).toHaveAttribute("title", /doppelten Ecken/);
  });

  test("the AI review banner names the status before and after (UX-207)", async ({ app, mcp }) => {
    const id = await createExample("hip", "Stav od AI");
    await openEditor(app, "Stav od AI");
    await mcp.call("update_job", { id, status: "ordered", summary: "Ordered." });
    await expect(app.page.locator(".review")).toContainText(`${cs("status.draft")} → ${cs("status.ordered")}`, { timeout: 10000 });
  });

  test("removing a mistyped long wall brings the drawing back to size (UX-203)", async ({ app, page }) => {
    await app.open();
    await page.locator("header").getByRole("button", { name: cs("lib.new") }).click();
    const nj = page.getByRole("dialog", { name: cs("new.title") });
    await nj.getByRole("tab", { name: cs("new.draw") }).click();
    await nj.getByRole("button", { name: cs("new.create") }).click();
    await clickPlan(page, 0, 0);
    const drawBar = bar(page, "draw.title");
    const len = drawBar.getByLabel(cs("draw.length"));
    for (const [v, k] of [["12,5", "ArrowRight"], ["4", "ArrowUp"], ["6,25", "ArrowLeft"], ["3", "ArrowUp"], ["99", "ArrowLeft"]]) {
      await len.fill(v);
      await len.press(k);
    }
    await drawBar.getByRole("button", { name: cs("draw.undo") }).click();
    const pts = page.locator(".view .wrap > svg circle.draft-pt");
    await expect(pts).toHaveCount(5);
    const xs: number[] = [];
    for (const c of await pts.all()) {
      const b = (await c.boundingBox())!;
      xs.push(b.x + b.width / 2);
    }
    const planBox = (await plan(page).boundingBox())!;
    // The 12.5 m drawing spans a good part of the plan again, not a 50 px blob.
    expect(Math.max(...xs) - Math.min(...xs)).toBeGreaterThan(planBox.width * 0.4);
  });
  test("at 600x420 a selected dormer's distances and the face labels do not pile up (UI-209)", async ({ app, page, mcp }) => {
    await page.setViewportSize({ width: 600, height: 420 });
    const id = await createExample("gable", "Vikýř malý");
    await app.open({ locale: "de", theme: "dark" });
    await app.openJob("Vikýř malý");
    await mcp.call("add_dormer", { id, kind: "gable", start: [2, 1], summary: "Dormer." });
    await accept(page);
    const bb = (await page.locator("g.dormer path.dormer-area").first().boundingBox())!;
    await page.mouse.click(bb.x + bb.width / 2, bb.y + bb.height / 2);
    await expect(page.getByTestId("place-dims")).toBeVisible();
    const overlapping = await page.evaluate(() => {
      const a = [...document.querySelectorAll(".view .wrap > svg text")]
        .map((e) => ({ t: (e.textContent ?? "").trim(), b: e.getBoundingClientRect() }))
        .filter((x) => x.t && x.b.width);
      const out: string[] = [];
      for (let i = 0; i < a.length; i++)
        for (let j = i + 1; j < a.length; j++) {
          const [p, q] = [a[i].b, a[j].b];
          if (p.left < q.right - 1 && q.left < p.right - 1 && p.top < q.bottom - 1 && q.top < p.bottom - 1) out.push(`${a[i].t} × ${a[j].t}`);
        }
      return out;
    });
    expect(overlapping).toEqual([]);
  });
});
