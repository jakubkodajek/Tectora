// Guards for the round-14 audit (2026-11-23), the core's half.
//
// HUNT-1301  Revert of the one AI change shown, with only the user's put-back
//            of the offer's figures after it: no "AI steps reverted too", the
//            put-back stays, the history line says one change
// ARCH-1302  a job another program made too deep to open while it is open:
//            the editor's save repairs it with its own copy (Jakub 2026-11-23)
import fs from "node:fs";
import path from "node:path";
import type { Page } from "@playwright/test";
import { bar, openEditor, selectWall, setField } from "./editor";
import { LIBRARY_DIR } from "./env";
import { bridge, createExample, expect, loadProject, noToast, test, waitForProject } from "./fixtures";
import { cs } from "./i18n";

// TECTORA_SHOTS=<folder>/ saves a screenshot after each test (fix evidence).
test.afterEach(async ({ page }, info) => {
  const dir = process.env.TECTORA_SHOTS;
  if (!dir) return;
  const id = info.title.match(/(HUNT|UX|UI|ERR|TEST|ARCH)-\d+/)?.[0] ?? "r14";
  const n = info.title.replace(/[^a-z0-9]+/gi, "-").slice(0, 40);
  await page.screenshot({ path: `${dir}r14-${id}-${n}-${process.env.TECTORA_SHOTS_TAG ?? "after"}.png`, timeout: 10_000 }).catch(() => {});
});

const bannerRevert = (page: Page) => page.locator(".review").getByRole("button", { name: cs("ai.revert") });

/** 12 November 2026, the day the "offer" was made. */
const OFFER_MS = Date.UTC(2026, 10, 12, 10, 0, 0);

/**
 * An offer job whose stored result an older engine (0.8.0) stored with a
 * roof area of 150 m², the AI's status change accepted (as audit-r9-recalc
 * `olderOffer`).
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

/** `{"a": {"a": … {}}}` with `n` objects inside each other. */
function nestedObjects(n: number): unknown {
  let v: unknown = {};
  for (let k = 1; k < n; k++) v = { a: v };
  return v;
}

test.describe("round 14: the core's half", () => {
  test("Revert of the one AI change shown, with the user's put-back after it, says nothing more and keeps the put-back (HUNT-1301)", async ({ app, page, mcp }) => {
    const name = "R14 vráceno";
    const id = await olderOffer(mcp, name);
    // The stored result recalculated (the engine's entry), an AI note, then the user's put-back of the offer's figures.
    expect(await bridge("refresh_stored_result", { id })).toBeTruthy();
    const engine = (await bridge<{ ts_ms: number; actor: string }[]>("project_activity", { id })).find((a) => a.actor === "tectora")!;
    await mcp.call("update_job", { id, notes: "Od AI", summary: "Poznámka od AI" });
    await waitForProject(id, (p) => p.notes === "Od AI");
    await bridge("restore_offer_result", { id, entryTsMs: engine.ts_ms });
    await waitForProject(id, (p) => p.result_snapshot?.engine_version === "0.8.0");

    await app.open({ prefs: { followAi: false } });
    await app.openJob(name);
    await app.saved();
    await expect(page.locator(".review")).toBeVisible();
    await bannerRevert(page).click();
    const back = await waitForProject(id, (p) => !p.notes, 8000);
    await app.saved();
    await noToast(app.toasts().filter({ hasText: cs("review.revertedMore", { n: 1 }) }), 1500, "a further AI step reported reverted");
    // The put-back stays, and the history says one change was reverted.
    expect(back.result_snapshot.engine_version).toBe("0.8.0");
    expect(back.result_snapshot.roof_area).toBe(150);
    const log = await bridge<{ ts_ms: number; summary: string }[]>("project_activity", { id });
    const last = log.reduce((a, b) => (b.ts_ms > a.ts_ms ? b : a));
    expect(last.summary).toBe("Reverted: Poznámka od AI");
  });

  test("a job another program made too deep to open while it is open is repaired by the editor's save (ARCH-1302)", async ({ app, page }) => {
    const id = await createExample("hip", "R14 hluboká");
    await openEditor(app, "R14 hluboká");
    await app.saved();
    const file = path.join(LIBRARY_DIR, `${id}.tectora`);
    const outside = JSON.parse(fs.readFileSync(file, "utf8"));
    outside.crm = nestedObjects(40);
    fs.writeFileSync(file, JSON.stringify(outside));
    await selectWall(page, 0);
    await setField(bar(page, "def.title").getByLabel(cs("edge.pitch")), 30);
    await expect
      .poll(() => {
        try {
          const p = JSON.parse(fs.readFileSync(file, "utf8"));
          return `${p.parts[0].edges[0].pitch_deg} ${"crm" in p}`;
        } catch {
          return null;
        }
      }, { timeout: 8000 })
      .toBe("30 false");
    await app.saved();
    expect((await loadProject(id)).name).toBe("R14 hluboká");
    await expect(app.toasts().filter({ hasText: cs("err.too_many") })).toHaveCount(0);
    // The live reload of the outside write may report it could not be read.
    app.errors.length = 0;
  });
});
