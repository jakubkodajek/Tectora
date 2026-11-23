// Guards for the round-10 audit (2026-11-23), the offer's stored result.
//
// HUNT-902 the offer's figures outlive the change log: after more changes
//          than the log keeps, the result card still says "recalculated"
//          and its own button puts the offer's figures back
// UX-902   a job past the draft without a stored result (dropped for an
//          impossible date) shows the result card "none stored" with the
//          store button the warning asks for
import fs from "node:fs";
import path from "node:path";
import type { Page } from "@playwright/test";
import { LIBRARY_DIR } from "./env";
import { bridge, createExample, expect, loadProject, test, waitForProject } from "./fixtures";
import { cs } from "./i18n";

// TECTORA_SHOTS=<folder>/ saves a screenshot after each test (fix evidence),
// named by the finding the test guards.
test.afterEach(async ({ page }, info) => {
  const dir = process.env.TECTORA_SHOTS;
  if (!dir) return;
  const id = info.title.match(/(HUNT|UX|TEST|ARCH|ERR)-\d+/)?.[0] ?? "r10";
  await page.screenshot({ path: `${dir}r10-${id}-${process.env.TECTORA_SHOTS_TAG ?? "after"}.png`, timeout: 10_000 }).catch(() => {});
});

const dockTab = (page: Page, key: string) => page.locator("aside .tabs").getByRole("tab", { name: cs(key) });
const putBack = (page: Page) => page.getByRole("button", { name: cs("snap.revertRefresh") });
/** 12 November 2026, the day the "offer" was made. */
const OFFER_MS = Date.UTC(2026, 10, 12, 10, 0, 0);

/**
 * An offer job whose stored result looks like one engine 0.8.0 stored with
 * a roof area of 150 m² (as audit-r9-recalc `olderOffer`), the AI's status
 * change accepted: the app's open recalculates it.
 */
async function olderOffer(mcp: any, name: string): Promise<string> {
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
  p.result_snapshot.roof_area = 150;
  p.result_snapshot.taken_ms = OFFER_MS;
  fs.writeFileSync(file, JSON.stringify(p));
  fs.writeFileSync(origins, JSON.stringify(o));
  const log = await bridge<{ ts_ms: number }[]>("project_activity", { id });
  await bridge("accept_changes", { id, tsMs: log.reduce((m, a) => Math.max(m, a.ts_ms), 0) });
  return id;
}

test.describe("round 10: the offer's stored result", () => {
  test("the result card puts the offer's figures back after the change log dropped the recalculation (HUNT-902)", async ({ app, page, mcp }) => {
    const id = await olderOffer(mcp, "Dlouhá historie");
    await app.open({ prefs: { followAi: false } });
    await app.openJob("Dlouhá historie");
    await waitForProject(id, (q) => q.result_snapshot?.engine_version !== "0.8.0");
    await app.saved();
    const rec = await bridge<{ entry_ts_ms: number } | null>("result_recalculation", { id });
    expect(rec, "recalculated on open").not.toBeNull();
    await page.getByRole("button", { name: cs("editor.back") }).click();

    // An AI's session of 55 logged changes: the engine's entry leaves the log.
    for (let i = 0; i < 55; i++) await mcp.call("update_job", { id, notes: `note ${i}`, summary: "Note" });
    const log = await bridge<{ ts_ms: number }[]>("project_activity", { id });
    expect(log.some((e) => e.ts_ms === rec!.entry_ts_ms), "the engine's entry is trimmed").toBe(false);

    await app.openJob("Dlouhá historie");
    await app.saved();
    await dockTab(page, "dock.ai").click();
    await expect(page.getByTestId("history-entry").first()).toBeVisible();
    await expect(putBack(page)).toHaveCount(0);
    await dockTab(page, "dock.results").click();
    await expect(page.getByTestId("snap-offer")).toHaveText(cs("snap.offerStored", { date: new Date(OFFER_MS).toLocaleDateString("cs-CZ") }));
    await page.getByTestId("snap-restore").click();
    const back = await waitForProject(id, (q) => q.result_snapshot?.engine_version === "0.8.0");
    expect(back.result_snapshot.roof_area).toBe(150);
    expect(back.result_snapshot.taken_ms).toBe(OFFER_MS);
    expect(back.notes).toBe("note 54");
    await app.saved();
    await expect(page.getByTestId("snap-restore")).toHaveCount(0);
    await expect(page.getByTestId("snap-offer")).toHaveCount(0);
    expect(await bridge("result_recalculation", { id })).toBeNull();
  });

  test("an ordered job whose stored result was left out shows the card with the store button (UX-902)", async ({ app, page }) => {
    const id = await createExample("gable", "Bez výsledku");
    const file = path.join(LIBRARY_DIR, `${id}.tectora`);
    const p = JSON.parse(fs.readFileSync(file, "utf8"));
    p.job.status = "ordered";
    p.result_snapshot = { engine_version: "0.9.0", taken_ms: 4242424242, roof_area: 99, ridge_height: 5, lines_by_code: { T: 20 } };
    // An impossible date, as another tool would write it.
    fs.writeFileSync(file, JSON.stringify(p).replace("4242424242", "8640000000000001"));

    await app.open({ prefs: { followAi: false } });
    await app.openJob("Bez výsledku");
    await expect(app.toasts().filter({ hasText: cs("err.result_dropped") })).toHaveCount(1);
    await dockTab(page, "dock.results").click();
    const card = page.getByTestId("snap-none");
    await expect(card).toContainText(cs("snap.none"));
    await expect(page.getByRole("heading", { name: cs("snap.title") })).toBeVisible();
    await card.getByRole("button", { name: cs("snap.store") }).click();
    const stored = await waitForProject(id, (q) => !!q.result_snapshot);
    expect(stored.job.status).toBe("ordered");
    expect(stored.result_snapshot.taken_ms).toBeLessThan(8_640_000_000_000_000);
    await app.saved();
    await expect(page.getByTestId("snap-none")).toHaveCount(0);
    await expect(page.getByTestId("snap-state")).toContainText(cs("snap.same"));
    expect((await loadProject(id)).result_snapshot).toBeTruthy();
  });

  test("a draft without a stored result shows no result card (UX-902)", async ({ app, page }) => {
    await createExample("gable", "Rozpracovaná");
    await app.open({ prefs: { followAi: false } });
    await app.openJob("Rozpracovaná");
    await app.saved();
    await dockTab(page, "dock.results").click();
    await expect(page.getByTestId("res-legend")).toBeVisible();
    await expect(page.getByTestId("snap-none")).toHaveCount(0);
    await expect(page.getByRole("heading", { name: cs("snap.title") })).toHaveCount(0);
  });
});
