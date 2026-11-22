// Guards for the round-9 audit (2026-11-22), "Design A": the recalculation
// of an offer's stored result is one fact in the core (the job's stored
// result is the engine's while its origin says so), and everything the
// editor shows about it comes from that fact, not from "the newest entry of
// the change log".
//
// ERR-801  the put-back button acts on its own entry, also when an AI saved
//          just before the press; Revert refuses another entry (revert_stale)
// ERR-802  a result the user stored since shows no "recalculated", no button
// UX-802   the button stays after an AI save, and after that save's revert
// UI-802   Undo across the put-back shows "recalculated" and the button again
// UI-803   the user's put-back is not "AI changes reverted"
// UI-804   an opened history entry stays open when a new entry arrives
// UI-801   the notice's button opens the panel in a narrow window
// HUNT-801 a notice of a job that was left is not shown in the next editor
// ARCH-802 an "engine" entry this library did not write: no line, no button
// UX-806   opening an offer keeps it in the library's "changed by AI" list
// UX-807   a recalculation of line lengths only names the line codes
// ERR-803  the editor says it is opening the job; the tools wait
// TEST-803 the library's job panel names Tectora (Home uses actorLabel)
// TEST-804 the card's "recalculated" line after an edit
import fs from "node:fs";
import path from "node:path";
import type { Locator, Page } from "@playwright/test";
import { bar, selectWall } from "./editor";
import { LIBRARY_DIR } from "./env";
import { bridge, createExample, expect, listProjects, loadProject, test, waitForProject } from "./fixtures";
import { cs } from "./i18n";

// TECTORA_SHOTS=<folder>/ saves a screenshot after each test (fix evidence).
test.afterEach(async ({ page }, info) => {
  const dir = process.env.TECTORA_SHOTS;
  if (!dir) return;
  const n = info.title.replace(/[^a-z0-9]+/gi, "-").slice(0, 48);
  await page.screenshot({ path: `${dir}r9-recalc-${n}-${process.env.TECTORA_SHOTS_TAG ?? "after"}.png`, timeout: 10_000 }).catch(() => {});
});

const dockTab = (page: Page, key: string) => page.locator("aside .tabs").getByRole("tab", { name: cs(key) });
const entries = (page: Page) => page.getByTestId("history-entry");
const putBack = (page: Page) => page.getByRole("button", { name: cs("snap.revertRefresh") });
const recalcToast = (app: { toasts(): Locator }) => app.toasts().filter({ hasText: "Tectora 0.8.0" });

/** 12 November 2026, the day the "offer" was made. */
const OFFER_MS = Date.UTC(2026, 10, 12, 10, 0, 0);
const czDate = (ms: number) => new Date(ms).toLocaleDateString("cs-CZ");
const cz = (n: number) => n.toLocaleString("cs-CZ", { minimumFractionDigits: 2, maximumFractionDigits: 2 });

async function typeInto(page: Page, field: Locator, text: string) {
  await field.click();
  await field.press("ControlOrMeta+a");
  await page.keyboard.type(text);
}

/**
 * An offer job whose stored result looks like one an older engine stored
 * (as audit-r8-ed `olderOffer`): engine 0.8.0, roof area `stored` m², taken
 * on OFFER_MS, its origin checked by 0.8.0 and the AI's status change
 * accepted. `lines`: the area stays the engine's and the first line length
 * is 2 m longer instead (UX-807).
 */
async function olderOffer(mcp: any, name: string, opts: { stored?: number; lines?: boolean } = {}): Promise<{ id: string; area: number; code: string }> {
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
  const code = Object.keys(p.result_snapshot.lines_by_code)[0];
  if (opts.lines) p.result_snapshot.lines_by_code[code] += 2;
  else p.result_snapshot.roof_area = opts.stored ?? 150;
  p.result_snapshot.taken_ms = OFFER_MS;
  fs.writeFileSync(file, JSON.stringify(p));
  fs.writeFileSync(origins, JSON.stringify(o));
  const log = await bridge<{ ts_ms: number }[]>("project_activity", { id });
  await bridge("accept_changes", { id, tsMs: log.reduce((m, a) => Math.max(m, a.ts_ms), 0) });
  return { id, area, code };
}

/** Opens the recalculated offer `name` from the library and waits for the notice and the save state. */
async function openRecalculated(app: any, name: string) {
  await app.open({ prefs: { followAi: false } });
  await app.openJob(name);
  await expect(recalcToast(app)).toHaveCount(1);
  await app.saved();
}

/** Holds the bridge's answers to `cmd` until the returned function is called. */
async function hold(page: Page, cmd: string, when: (body: any) => boolean = () => true): Promise<{ release: () => void; held: () => number }> {
  let open!: () => void;
  const gate = new Promise<void>((r) => (open = r));
  let n = 0;
  await page.route(`**/${cmd}`, async (route) => {
    let body: any = {};
    try {
      body = JSON.parse(route.request().postData() ?? "{}");
    } catch {
      // Not JSON: not ours to hold.
    }
    if (!when(body)) return route.continue();
    n++;
    const res = await route.fetch();
    await gate;
    await route.fulfill({ response: res }).catch(() => {});
  });
  return { release: open, held: () => n };
}

test.describe("round 9: the recalculation entry and its way back (Design A)", () => {
  test("the put-back button acts on its own entry when an AI saved just before the press, and Revert refuses another entry (ERR-801)", async ({ app, page, mcp }) => {
    const { id } = await olderOffer(mcp, "Závod");
    await openRecalculated(app, "Závod");
    await dockTab(page, "dock.ai").click();
    await expect(putBack(page)).toHaveCount(1);
    const engine = (await bridge<{ ts_ms: number; actor: string }[]>("project_activity", { id })).find((a) => a.actor === "tectora")!;

    // The panel no longer hears of new entries: what it shows is what the user presses on.
    const frozen = await hold(page, "project_activity");
    await mcp.call("update_job", { id, notes: "Zavolat zákazníkovi", summary: "Poznámka" });
    await waitForProject(id, (p) => p.notes === "Zavolat zákazníkovi");
    await putBack(page).click();
    const back = await waitForProject(id, (p) => p.result_snapshot?.engine_version === "0.8.0");
    // The offer's figures are back and the AI's note stays.
    expect(back.result_snapshot.roof_area).toBe(150);
    expect(back.notes).toBe("Zavolat zákazníkovi");
    frozen.release();
    await page.unroute("**/project_activity");
    const log = await bridge<{ actor: string; summary: string; restores?: number }[]>("project_activity", { id });
    expect(log[0]).toMatchObject({ actor: "user", restores: engine.ts_ms });
    expect(log[0].summary).toMatch(/^Offer result restored/);
    expect(log[1]).toMatchObject({ actor: "Claude", summary: "Poznámka" });

    // Revert names its entry: with the engine's entry's time, the AI's newer change is not undone.
    await expect(bridge("revert_last_change", { id, expectTs: engine.ts_ms })).rejects.toThrow(/revert_stale/);
    expect((await loadProject(id)).notes).toBe("Zavolat zákazníkovi");
    // And the offer's figures cannot be put back twice.
    await expect(bridge("restore_offer_result", { id, entryTsMs: engine.ts_ms })).rejects.toThrow(/revert_stale/);
  });

  test("Revert on an AI entry does nothing and says so when a second AI change landed before the press (ERR-801)", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "Dvě změny");
    await app.open({ prefs: { followAi: false } });
    await app.openJob("Dvě změny");
    await app.saved();
    await mcp.call("update_job", { id, notes: "první", summary: "První" });
    await dockTab(page, "dock.ai").click();
    const revert = entries(page).first().getByRole("button", { name: cs("ai.revert") });
    await expect(revert).toBeVisible();
    await expect(entries(page).first()).toContainText("První");

    const frozen = await hold(page, "project_activity");
    await mcp.call("update_job", { id, number: "2026-9", summary: "Druhá" });
    await waitForProject(id, (p) => p.job.number === "2026-9");
    // Still drawn on "První": the press must not undo "Druhá" in its place.
    await revert.click();
    await expect(app.toasts().filter({ hasText: "revert_stale" })).toHaveCount(1);
    await expect(app.toasts().filter({ hasText: "revert_stale" })).toContainText(cs("err.revert_stale"));
    // The refusal is logged to the console as every shown error is: expected here.
    expect(app.errors.every((e: string) => e.includes("revert_stale"))).toBe(true);
    app.errors.length = 0;
    frozen.release();
    await page.unroute("**/project_activity");
    const now = await loadProject(id);
    expect([now.notes, now.job.number]).toEqual(["první", "2026-9"]);
    const log = await bridge<{ summary: string }[]>("project_activity", { id });
    expect(log.map((a) => a.summary).slice(0, 2)).toEqual(["Druhá", "První"]);
    // Read again, the button is on the newest entry and undoes that one.
    await expect(entries(page).first()).toContainText("Druhá");
    await entries(page).first().getByRole("button", { name: cs("ai.revert") }).click();
    await waitForProject(id, (p) => p.job.number === "");
    expect((await loadProject(id)).notes).toBe("první");
  });

  test("an edit keeps the card's recalculated line; a result the user stores takes the line and the button away; Undo shows them again (ERR-802, TEST-804, UI-802)", async ({ app, page, mcp }) => {
    const { id } = await olderOffer(mcp, "Později");
    await openRecalculated(app, "Později");
    const now = await loadProject(id);
    const to = now.result_snapshot.engine_version as string;
    const line = cs("snap.recalculated", { date: czDate(now.result_snapshot.taken_ms), from: "0.8.0", to });

    // TEST-804: the design is edited; the stored result is still the engine's.
    await selectWall(page, 0);
    await typeInto(page, bar(page, "def.title").getByLabel(cs("edge.pitch")), "33");
    await page.keyboard.press("Enter");
    await waitForProject(id, (q) => q.parts[0].edges.some((e: any) => e.pitch_deg === 33));
    await app.saved();
    await dockTab(page, "dock.results").click();
    await expect(page.getByTestId("snap-state")).toContainText(cs("snap.changed"));
    await expect(page.getByTestId("snap-recalculated")).toHaveText(line);
    await expect(page.getByTestId("snap-offer")).toHaveText(cs("snap.offerStored", { date: czDate(OFFER_MS) }));

    // ERR-802: the user stores the result of the changed roof.
    await page.getByRole("button", { name: cs("snap.store") }).click();
    const stored = await waitForProject(id, (q) => q.result_snapshot?.taken_ms > now.result_snapshot.taken_ms);
    await app.saved();
    await expect(page.getByTestId("snap-state")).toContainText(cs("snap.same"));
    await expect(page.getByTestId("snap-state")).toContainText(czDate(stored.result_snapshot.taken_ms));
    await expect(page.getByTestId("snap-recalculated")).toHaveCount(0);
    await expect(page.getByTestId("snap-offer")).toHaveCount(0);
    await dockTab(page, "dock.ai").click();
    await expect(entries(page).first()).toContainText("Tectora");
    await expect(putBack(page)).toHaveCount(0);
    expect(await bridge("result_recalculation", { id })).toBeNull();

    // Opened again: the same, and nothing to fail with an error.
    await page.getByRole("button", { name: cs("editor.back") }).click();
    await app.openJob("Později");
    await app.saved();
    await dockTab(page, "dock.ai").click();
    await expect(entries(page).first()).toContainText("Tectora");
    await expect(putBack(page)).toHaveCount(0);
    await dockTab(page, "dock.results").click();
    await expect(page.getByTestId("snap-offer")).toHaveCount(0);
    await expect(app.toasts()).toHaveCount(0);
  });

  test("Undo across the put-back brings the recalculated result back in sight: the card line and the button return (UI-802)", async ({ app, page, mcp }) => {
    const { id } = await olderOffer(mcp, "Zpět a zpět");
    await openRecalculated(app, "Zpět a zpět");
    const engines = (await loadProject(id)).result_snapshot;
    await dockTab(page, "dock.ai").click();
    await putBack(page).click();
    await waitForProject(id, (p) => p.result_snapshot?.engine_version === "0.8.0");
    await app.saved();
    await expect(putBack(page)).toHaveCount(0);
    // UI-803: the user's put-back is no "AI changes reverted", in the panel and in the log.
    await expect(entries(page).first()).toContainText(cs("ai.you"));
    await expect(entries(page).first()).toContainText(cs("chg.resultRestored", { to: engines.engine_version, from: "0.8.0" }));
    await expect(entries(page).first()).not.toContainText(cs("ai.revertedEntry"));
    await dockTab(page, "dock.results").click();
    await expect(page.getByTestId("snap-offer")).toHaveCount(0);

    // Ctrl+Z: the recalculated figures are the stored result again, and the editor says so.
    await page.locator(".view .wrap > svg").first().focus().catch(() => {});
    await page.keyboard.press("ControlOrMeta+z");
    const again = await waitForProject(id, (p) => p.result_snapshot?.engine_version === engines.engine_version);
    expect(again.result_snapshot.taken_ms).toBe(engines.taken_ms);
    await app.saved();
    await expect(page.getByTestId("snap-state")).toContainText(cs("snap.recalculated", { date: czDate(engines.taken_ms), from: "0.8.0", to: engines.engine_version }));
    await expect(page.getByTestId("snap-offer")).toHaveText(cs("snap.offerStored", { date: czDate(OFFER_MS) }));
    await dockTab(page, "dock.ai").click();
    await expect(putBack(page)).toHaveCount(1);
    // And it works again: the offer's figures, for good.
    await putBack(page).click();
    await waitForProject(id, (p) => p.result_snapshot?.engine_version === "0.8.0");
    await app.saved();
    await page.getByRole("button", { name: cs("editor.back") }).click();
    await app.openJob("Zpět a zpět");
    await app.saved();
    await expect(recalcToast(app)).toHaveCount(0);
    expect((await loadProject(id)).result_snapshot.roof_area).toBe(150);
  });

  test("the put-back button stays after an AI save and after that save's revert, and an opened entry stays open (UX-802, UI-804)", async ({ app, page, mcp }) => {
    const { id } = await olderOffer(mcp, "Po AI");
    await openRecalculated(app, "Po AI");
    const overhang = (await loadProject(id)).parts[0].edges[0].overhang as number;
    // The notice's button opens the engine's entry with its figures.
    await recalcToast(app).getByRole("button", { name: cs("snap.refreshedShow") }).click();
    await expect(page.getByTestId("refreshed-figures")).toBeVisible();

    await mcp.call("update_edges", { id, changes: [{ all: true, overhang: overhang + 0.1 }], summary: "Přesah okapu" });
    await expect(entries(page).first()).toContainText("Přesah okapu");
    // UI-804: the entry the user reads stays open, the new one is not opened in its place.
    await expect(page.getByTestId("refreshed-figures")).toBeVisible();
    await expect(entries(page).first().locator("ul")).toHaveCount(0);
    // UX-802: both ways back are offered, each on its own entry.
    await expect(entries(page).first().getByRole("button", { name: cs("ai.revert") })).toBeVisible();
    await expect(entries(page).nth(1).getByRole("button", { name: cs("snap.revertRefresh") })).toBeVisible();

    await entries(page).first().getByRole("button", { name: cs("ai.revert") }).click();
    await waitForProject(id, (p) => p.parts[0].edges[0].overhang === overhang);
    await app.saved();
    // UI-803: this one was an AI's change.
    await expect(entries(page).first()).toContainText(cs("ai.revertedEntry"));
    await expect(putBack(page)).toHaveCount(1);
    await putBack(page).click();
    const back = await waitForProject(id, (p) => p.result_snapshot?.engine_version === "0.8.0");
    expect(back.result_snapshot.roof_area).toBe(150);
    expect(back.parts[0].edges[0].overhang).toBe(overhang);
    await expect(putBack(page)).toHaveCount(0);
  });

  test("an engine entry this library did not write shows no recalculated line and no put-back button (ARCH-802)", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "Podvrh");
    await mcp.call("update_job", { id, status: "offer", summary: "Offer sent." });
    const p = await loadProject(id);
    const before = JSON.parse(JSON.stringify(p));
    before.result_snapshot.engine_version = "0.0.1";
    before.result_snapshot.roof_area = 300;
    before.result_snapshot.taken_ms -= 30 * 86_400_000;
    const log = path.join(LIBRARY_DIR, ".activity", `${id}.jsonl`);
    const last = fs.readFileSync(log, "utf8").trim().split("\n").map((l) => JSON.parse(l)).pop();
    const forged = {
      ts_ms: last.ts_ms + 1000,
      actor: "tectora",
      summary: `Stored result recalculated (Tectora 0.0.1 -> ${p.result_snapshot.engine_version}): roof area 300.00 -> ${p.result_snapshot.roof_area.toFixed(2)} m2`,
      changes: [],
      before,
      after: p,
    };
    fs.appendFileSync(log, `${JSON.stringify(forged)}\n`);
    expect(await bridge("result_recalculation", { id })).toBeNull();
    await expect(bridge("restore_offer_result", { id, entryTsMs: forged.ts_ms })).rejects.toThrow(/revert_stale/);

    await app.open({ prefs: { followAi: false } });
    await app.openJob("Podvrh");
    await app.saved();
    await dockTab(page, "dock.results").click();
    await expect(page.getByTestId("snap-state")).toBeVisible();
    await expect(page.getByTestId("snap-state")).not.toContainText("0.0.1");
    await expect(page.getByTestId("snap-recalculated")).toHaveCount(0);
    await expect(page.getByTestId("snap-offer")).toHaveCount(0);
    await dockTab(page, "dock.ai").click();
    await expect(entries(page).first()).toContainText("0.0.1");
    await expect(putBack(page)).toHaveCount(0);
    expect((await loadProject(id)).result_snapshot.roof_area).toBe(p.result_snapshot.roof_area);
  });

  test("the notice of a job that was left is not shown in the next job's editor (HUNT-801)", async ({ app, page, mcp }) => {
    const slow = await olderOffer(mcp, "Pomalá", { stored: 170 });
    await olderOffer(mcp, "Rychlá");
    // The first job's recalculation answers only after the user is in the second job.
    const held = await hold(page, "refresh_stored_result", (b) => b.id === slow.id);
    await app.open({ prefs: { followAi: false } });
    await page.getByRole("row").filter({ hasText: "Pomalá" }).first().dblclick();
    await expect.poll(() => held.held()).toBe(1);
    await page.getByRole("button", { name: cs("editor.back") }).first().click();
    await app.openJob("Rychlá");
    await expect(recalcToast(app)).toHaveCount(1);
    await expect(recalcToast(app)).toContainText(`${cz(150)} →`);
    held.release();
    // The held answer has come and gone: still one notice, the second job's.
    // Looked at once, without waiting: a notice that goes away by itself after a few seconds is still a wrong notice.
    await page.waitForTimeout(1200);
    expect(await app.toasts().filter({ hasText: cz(170) }).count()).toBe(0);
    expect(await app.toasts().allInnerTexts()).toHaveLength(1);
    await expect(page.locator(".editor header input.title")).toHaveValue("Rychlá");
    // The job that was left was recalculated all the same; its next open says nothing new.
    expect((await loadProject(slow.id)).result_snapshot.engine_version).not.toBe("0.8.0");
  });

  test("in a narrow window the notice's button opens the panel with the old figures (UI-801)", async ({ app, page, mcp }) => {
    await olderOffer(mcp, "Úzké okno");
    await page.setViewportSize({ width: 800, height: 600 });
    await openRecalculated(app, "Úzké okno");
    await expect(page.locator("aside .dock")).toBeHidden();
    await recalcToast(app).getByRole("button", { name: cs("snap.refreshedShow") }).click();
    await expect(page.locator("aside .dock")).toBeVisible();
    await expect(dockTab(page, "dock.ai")).toHaveAttribute("aria-selected", "true");
    await expect(page.getByTestId("refreshed-figures")).toBeVisible();
    await expect(putBack(page)).toBeVisible();
  });

  test("a recalculation that changed line lengths only names the lines, not an unchanged area (UX-807)", async ({ app, page, mcp }) => {
    const { id, area, code } = await olderOffer(mcp, "Jen linie", { lines: true });
    await app.open({ prefs: { followAi: false } });
    await app.openJob("Jen linie");
    const toast = recalcToast(app);
    await expect(toast).toHaveCount(1);
    const to = (await loadProject(id)).result_snapshot.engine_version as string;
    await expect(toast).toContainText(cs("snap.refreshedNoticeLines", { from: "0.8.0", to, codes: code }));
    await expect(toast).not.toContainText(`${cz(area)} → ${cz(area)}`);
    await toast.getByRole("button", { name: cs("snap.refreshedShow") }).click();
    await expect(entries(page).first()).toContainText(cs("chg.resultRefreshedLines", { from: "0.8.0", to, codes: code }));
    await expect(page.getByTestId("refreshed-figures").locator("li")).toHaveCount(1);
    await expect(page.getByTestId("refreshed-figures").locator("li").first()).toContainText(code);
  });

  test("opening an offer keeps it in the library's changed-by-AI list, and the job panel names Tectora (UX-806, TEST-803)", async ({ app, page, mcp }) => {
    const { id } = await olderOffer(mcp, "Od AI");
    await app.open({ prefs: { followAi: false } });
    const row = page.getByRole("row").filter({ hasText: "Od AI" }).first();
    const aiFilter = page.getByRole("button", { name: new RegExp(`^${cs("lib.byAi")}`) });
    await expect(row.getByTestId("ai-tag")).toHaveText("Claude");
    await expect(aiFilter.locator("span")).toHaveText("1");

    await app.openJob("Od AI");
    await expect(recalcToast(app)).toHaveCount(1);
    await app.saved();
    await page.getByRole("button", { name: cs("editor.back") }).first().click();
    // The engine's entry is the last one now; the job is still one an AI changed.
    await expect(row.getByTestId("ai-tag")).toHaveText("Claude");
    await expect(aiFilter.locator("span")).toHaveText("1");
    await aiFilter.click();
    await expect(page.getByRole("row").filter({ hasText: "Od AI" })).toHaveCount(1);
    const card = (await listProjects()).find((p) => p.id === id);
    expect(card.last_activity.actor).toBe("tectora");
    expect(card.changed_by_ai).toMatchObject({ actor: "Claude", summary: "Offer sent." });
    // The same answer over MCP.
    const listed = await mcp.call("list_projects", { by_ai: true });
    expect(JSON.stringify(listed)).toContain(id);

    // TEST-803: the job panel credits the newest entry to Tectora, not to the user.
    await row.click();
    const last = page.getByTestId("detail-last");
    await expect(last).toBeVisible();
    await expect(last).toContainText("Tectora");
    await expect(last).not.toContainText(cs("ai.you"));
  });

  test("while the job is read the editor says so and its tools wait (ERR-803)", async ({ app, page }) => {
    const id = await createExample("gable", "Čekání");
    const held = await hold(page, "refresh_stored_result", (b) => b.id === id);
    await app.open({ prefs: { followAi: false } });
    await page.getByRole("row").filter({ hasText: "Čekání" }).first().dblclick();
    await expect.poll(() => held.held()).toBe(1);
    await expect(page.getByTestId("editor-loading")).toHaveText(cs("editor.loading"));
    await expect(page.getByTestId("editor-loading").locator("span")).toBeVisible();
    const toolbar = page.getByRole("toolbar").first();
    await expect(toolbar).toHaveAttribute("inert", "");
    await expect(toolbar).toHaveAttribute("aria-busy", "true");
    // A press on Standard changes nothing: no dialog, no second building later.
    await page.getByRole("button", { name: cs("tool.standard") }).click({ force: true, timeout: 2000 }).catch(() => {});
    await expect(page.getByRole("dialog")).toHaveCount(0);
    // The way out works meanwhile.
    await expect(page.getByRole("button", { name: cs("editor.back") }).first()).toBeEnabled();
    held.release();
    await expect(page.locator(".editor .statusbar")).toBeVisible();
    await expect(page.getByTestId("editor-loading")).toHaveCount(0);
    await expect(toolbar).not.toHaveAttribute("inert", "");
    await app.saved();
    expect((await loadProject(id)).parts).toHaveLength(1);
  });

  test("a draft, and an offer with nothing to recalculate, answer the open at once and leave no lock behind (ERR-803)", async ({ app: _app, mcp }) => {
    const draft = await createExample("gable", "Koncept");
    const lock = (id: string) => path.join(LIBRARY_DIR, ".activity", `${id}.lock`);
    fs.rmSync(lock(draft), { force: true });
    expect(await bridge("refresh_stored_result", { id: draft })).toBeNull();
    // Nothing was due, so the job's lock was not even taken.
    expect(fs.existsSync(lock(draft))).toBe(false);
    const { id } = await olderOffer(mcp, "Nabídka");
    const done = await bridge<{ entry_ts_ms: number; summary: string } | null>("refresh_stored_result", { id });
    expect(done?.summary).toMatch(/^Stored result recalculated \(Tectora 0\.8\.0 -> /);
    expect((await bridge<any>("result_recalculation", { id })).entry_ts_ms).toBe(done?.entry_ts_ms);
    fs.rmSync(lock(id), { force: true });
    expect(await bridge("refresh_stored_result", { id })).toBeNull();
    expect(fs.existsSync(lock(id))).toBe(false);
  });
});
