// Guards for the round-8 audit (2026-11-22), editor side.
//
// UX-702a/b/c: opening a job whose stored offer result an older engine
// made recalculates it (owner decision 2026-11-21). The editor says so in
// one neutral notice that leads to the history entry, the entry lists each
// figure old -> new, and the result card says "recalculated" with the
// offer's own date instead of "unchanged since stored" with the date of the
// open. UX-702d (owner decision 2026-11-22): the engine's entry has a
// Revert that puts the offer's figures back for good.
import fs from "node:fs";
import path from "node:path";
import type { Page } from "@playwright/test";
import { LIBRARY_DIR } from "./env";
import { bridge, createExample, expect, loadProject, test, waitForProject } from "./fixtures";
import { cs } from "./i18n";

const dockTab = (page: Page, key: string) => page.locator("aside .tabs").getByRole("tab", { name: cs(key) });

/** 12 November 2026, the day the "offer" was made. */
const OFFER_MS = Date.UTC(2026, 10, 12, 10, 0, 0);
const czDate = (ms: number) => new Date(ms).toLocaleDateString("cs-CZ");

/**
 * An offer job whose stored result looks like one an older engine stored:
 * engine 0.8.0, roof area 150 m², taken on OFFER_MS, its origin checked by
 * 0.8.0 and the AI's status change accepted.
 */
async function olderOffer(mcp: any, name: string): Promise<{ id: string; area: number }> {
  const id = await createExample("gable", name);
  await mcp.call("update_job", { id, status: "offer", summary: "Offer sent." });
  const file = path.join(LIBRARY_DIR, `${id}.tectora`);
  const p = JSON.parse(fs.readFileSync(file, "utf8"));
  expect(p.result_snapshot, "the offer stored a result").toBeTruthy();
  const area = p.result_snapshot.roof_area as number;
  const origins = path.join(LIBRARY_DIR, ".activity", `${id}.result.json`);
  const o = JSON.parse(fs.readFileSync(origins, "utf8")) as { taken_ms: number; checked: string }[];
  const taken = p.result_snapshot.taken_ms;
  for (const x of o) {
    if (x.taken_ms === taken) x.taken_ms = OFFER_MS;
    x.checked = "0.8.0";
  }
  p.result_snapshot.engine_version = "0.8.0";
  p.result_snapshot.roof_area = 150;
  p.result_snapshot.taken_ms = OFFER_MS;
  fs.writeFileSync(file, JSON.stringify(p));
  fs.writeFileSync(origins, JSON.stringify(o));
  const log = await bridge<{ ts_ms: number }[]>("project_activity", { id });
  await bridge("accept_changes", { id, tsMs: log.reduce((m, a) => Math.max(m, a.ts_ms), 0) });
  return { id, area };
}

test.describe("round 8: a stored result recalculated on open (UX-702)", () => {
  test("the editor tells it once, leads to the old figures and keeps the offer's date", async ({ app, page, mcp }) => {
    const { id, area } = await olderOffer(mcp, "Přepočet");
    // Opened from the library (follow mode would open it on its own).
    await app.open({ prefs: { followAi: false } });
    await app.openJob("Přepočet");
    const now = await loadProject(id);
    expect(now.result_snapshot.roof_area).toBeCloseTo(area, 2);
    const to = now.result_snapshot.engine_version as string;
    const a = (150).toLocaleString("cs-CZ", { minimumFractionDigits: 2, maximumFractionDigits: 2 });
    const b = area.toLocaleString("cs-CZ", { minimumFractionDigits: 2, maximumFractionDigits: 2 });

    // (c) One neutral notice with a way to the history entry.
    const toast = app.toasts().filter({ hasText: "Tectora 0.8.0" });
    await expect(toast).toHaveCount(1);
    await expect(toast).toContainText(`${a} → ${b}`);

    // (b) The card: recalculated today by which engine, and the offer's own date.
    await dockTab(page, "dock.results").click();
    const state = page.getByTestId("snap-state");
    await expect(state).toContainText(cs("snap.recalculated", { date: czDate(now.result_snapshot.taken_ms), from: "0.8.0", to }));
    await expect(state).not.toContainText(cs("snap.same"));
    await expect(page.getByTestId("snap-offer")).toHaveText(cs("snap.offerStored", { date: czDate(OFFER_MS) }));

    // (a) The notice's button opens the entry with each figure old -> new.
    await toast.getByRole("button", { name: cs("snap.refreshedShow") }).click();
    await expect(dockTab(page, "dock.ai")).toHaveAttribute("aria-selected", "true");
    const figures = page.getByTestId("refreshed-figures");
    await expect(figures).toBeVisible();
    await expect(figures.locator("li").first()).toContainText(`${cs("res.area")} ${a} → ${b}`);

    // Opened again: nothing more to recalculate, no second notice.
    await page.getByRole("button", { name: cs("editor.back") }).click();
    await app.openJob("Přepočet");
    await app.saved();
    await expect(app.toasts().filter({ hasText: "Tectora 0.8.0" })).toHaveCount(0);
  });

  test("the job follow mode opens is recalculated and told the same way", async ({ app, page, mcp }) => {
    const other = await createExample("hip", "Jiná");
    const { id } = await olderOffer(mcp, "Sledovaná");
    await app.open({ prefs: { followAi: false } });
    await app.openJob("Jiná");
    // Follow mode on only now: the offer above is not opened on its own.
    await page.keyboard.press("ControlOrMeta+,");
    const dlg = page.getByRole("dialog");
    await dlg.getByRole("checkbox", { name: new RegExp(cs("settings.followAi")) }).check();
    await dlg.getByRole("button", { name: cs("common.close") }).click();
    await mcp.call("show_project", { id });
    await expect(page.locator(".editor header")).toContainText("Sledovaná");
    await expect(app.toasts().filter({ hasText: "Tectora 0.8.0" })).toHaveCount(1);
    expect((await loadProject(id)).result_snapshot.engine_version).not.toBe("0.8.0");
    void other;
  });

  test("the engine's entry puts the offer's own figures back, and the next open leaves them (UX-702d)", async ({ app, page, mcp }) => {
    const { id } = await olderOffer(mcp, "Zpět");
    await app.open({ prefs: { followAi: false } });
    await app.openJob("Zpět");
    await expect(app.toasts().filter({ hasText: "Tectora 0.8.0" })).toHaveCount(1);
    await app.saved();
    await dockTab(page, "dock.ai").click();
    await page.getByRole("button", { name: cs("snap.revertRefresh") }).click();
    const back = await waitForProject(id, (p) => p.result_snapshot?.engine_version === "0.8.0");
    expect(back.result_snapshot.roof_area).toBe(150);
    expect(back.result_snapshot.taken_ms).toBe(OFFER_MS);
    await app.saved();
    await page.getByRole("button", { name: cs("editor.back") }).click();
    await app.openJob("Zpět");
    await app.saved();
    await expect(app.toasts().filter({ hasText: "Tectora 0.8.0" })).toHaveCount(0);
    expect((await loadProject(id)).result_snapshot.roof_area).toBe(150);
  });
});
