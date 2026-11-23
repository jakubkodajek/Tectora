// Guards for the round-11 audit (2026-11-23), the editor's half:
//
// design A1 (owner decision; HUNT-1001, UX-1001, UX-1002, ERR-1004, TEST-1001,
//          TEST-1003): Revert, Revert all and the put-back of the offer's
//          figures (AI tab and result card) are a short transaction like a
//          drag. While the call is out every edit is refused (same field +
//          Enter, another field + Enter, a value typed without Enter, a
//          confirmed building delete, Ctrl+Z); the answer is the job, the
//          file and the screen agree whenever the status says "Uloženo", and
//          the value typed afterwards lands within 3 s
// UI-1001  the result card's put-back and Store leave the keyboard's focus on
//          the card
// UI-1002  a job's notice closes with its editor and names the job
// UI-1003  a long job name wraps in the toast's title
// TEST-1005 a missed notice is not shown once a new result was stored
// UX-1003  a received recalculated offer says so on its card, without the
//          put-back; the sender is told the offer's figures stay with him
import type { Locator, Page } from "@playwright/test";
import { bar, selectWall } from "./editor";
import { FILES_DIR, LIBRARY_DIR } from "./env";
import { bridge, createExample, expect, loadProject, noToast, test, waitForProject } from "./fixtures";
import { cs } from "./i18n";
import { acceptAll, holdLock, olderOffer, timeline } from "./review";
import fs from "node:fs";
import path from "node:path";

// TECTORA_SHOTS=<folder>/ saves a screenshot after each test (fix evidence).
test.afterEach(async ({ page }, info) => {
  const dir = process.env.TECTORA_SHOTS;
  if (!dir) return;
  const id = info.title.match(/(HUNT|UX|UI|ERR|TEST)-\d+/)?.[0] ?? "r11";
  const n = info.title.replace(/[^a-z0-9]+/gi, "-").slice(0, 40);
  await page.screenshot({ path: `${dir}r11-${id}-${n}-${process.env.TECTORA_SHOTS_TAG ?? "after"}.png`, timeout: 10_000 }).catch(() => {});
});

const dockTab = (page: Page, key: string) => page.locator("aside .tabs").getByRole("tab", { name: cs(key) });
const entries = (page: Page) => page.getByTestId("history-entry");
const putBack = (page: Page) => page.getByRole("button", { name: cs("snap.revertRefresh") });
const recalcToast = (app: { toasts(): Locator }) => app.toasts().filter({ hasText: "Tectora 0.8.0" });
const status = (page: Page) => page.locator(".statusbar .save");
const field = (page: Page, key: "edge.pitch" | "edge.overhang") => bar(page, "def.title").getByLabel(cs(key));
const notesField = (page: Page) => page.locator("aside .job").getByLabel(cs("job.notes"));

type Action = "revert" | "revert-all" | "put-back-ai" | "put-back-card";
type Change = "add_part" | "overhang" | "notes" | "offer";
type Intruder = "same-enter" | "other-enter" | "typed-no-enter" | "delete" | "undo";

const CMD: Record<Action, string> = {
  revert: "revert_last_change",
  "revert-all": "revert_changes",
  "put-back-ai": "restore_offer_result",
  "put-back-card": "restore_offer_result",
};

/** What a field intruder types where. */
interface Target {
  /** The wall the bar shows (global index for `selectWall`), and its part and edge in the job. */
  wall: number;
  part: number;
  edge: number;
  same: "edge.pitch" | "edge.overhang" | "notes";
  other: "edge.pitch" | "edge.overhang";
}

const VALUE = { "edge.pitch": "33", "edge.overhang": "0,7", notes: "Moje poznámka" } as const;
const NUMBER = { "edge.pitch": 33, "edge.overhang": 0.7 } as const;

/** What the screen shows and what the file holds, in the same terms; a field not on screen is left out of both. */
async function states(page: Page, id: string, t: Target) {
  const text = (await page.locator(".statusbar").textContent()) ?? "";
  const m = text.match(/Stěny: (\d+) · budovy: (\d+)/);
  const num = async (l: Locator) => ((await l.count()) ? Math.round(Number((await l.inputValue()).replace(",", ".")) * 100) / 100 : null);
  const screen = {
    walls: m ? Number(m[1]) : null,
    buildings: m ? Number(m[2]) : null,
    pitch: await num(field(page, "edge.pitch")),
    overhang: await num(field(page, "edge.overhang")),
    notes: (await notesField(page).count()) ? await notesField(page).inputValue() : null,
  };
  const p = await loadProject(id);
  const e = p.parts[t.part]?.edges[t.edge];
  const file = {
    walls: p.parts.reduce((n: number, q: any) => n + q.edges.length, 0),
    buildings: p.parts.length,
    pitch: screen.pitch === null || !e ? null : Math.round(e.pitch_deg * 100) / 100,
    overhang: screen.overhang === null || !e ? null : Math.round(e.overhang * 100) / 100,
    notes: screen.notes === null ? null : (p.notes ?? ""),
  };
  return { screen, file };
}

/**
 * Samples the status every 50 ms and records every sample that says
 * "Uloženo" while the screen and the file differ (TEST-1003: the guard of
 * round 10 passed at its first poll).
 */
/** Samples the status watcher must have taken to tell anything. */
const MIN_SAMPLES = 5;

function watchSaved(page: Page, id: string, t: Target) {
  let on = true;
  let samples = 0;
  const bad: string[] = [];
  const run = (async () => {
    while (on) {
      const before = (await status(page).textContent().catch(() => "")) ?? "";
      if (/Uloženo/.test(before)) {
        const { screen, file } = await states(page, id, t);
        const after = (await status(page).textContent().catch(() => "")) ?? "";
        if (/Uloženo/.test(after) && JSON.stringify(screen) !== JSON.stringify(file)) bad.push(`screen ${JSON.stringify(screen)} file ${JSON.stringify(file)}`);
      }
      samples++;
      await page.waitForTimeout(50);
    }
  })();
  return async () => {
    // Under load one sample (a file read and two status reads) can take
    // longer than the wait it was given: keep sampling, the screen staying
    // as it is, until enough samples were taken (at most 20 s more), rather
    // than ending with too few to tell anything.
    const end = Date.now() + 20_000;
    while (samples <= MIN_SAMPLES && Date.now() < end) await page.waitForTimeout(50);
    on = false;
    await run;
    return { bad, samples };
  };
}

/** Clicks `l` where it is drawn, as a user would, also when it is inert (a locator click would wait for it). False when it is not drawn. */
async function mouseClick(page: Page, l: Locator): Promise<boolean> {
  if (!(await l.count())) return false;
  const box = await l.first().boundingBox();
  if (!box) return false;
  await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
  return true;
}

async function intrude(page: Page, app: any, intruder: Intruder, t: Target) {
  const into = async (which: Target["same"], enter: boolean) => {
    if (which === "notes") {
      await mouseClick(page, dockTab(page, "dock.job"));
      await mouseClick(page, notesField(page));
    } else {
      await mouseClick(page, field(page, which));
      await page.keyboard.press("ControlOrMeta+a");
    }
    await page.keyboard.type(VALUE[which]);
    if (enter) await page.keyboard.press(which === "notes" ? "Tab" : "Enter");
  };
  if (intruder === "same-enter") await into(t.same, true);
  else if (intruder === "other-enter") await into(t.other, true);
  else if (intruder === "typed-no-enter") await into(t.same === "notes" ? t.other : t.same, false);
  else if (intruder === "delete") {
    await app.answerConfirm(true, true);
    await mouseClick(page, page.getByRole("toolbar").first().getByRole("button", { name: cs("tool.deletePart"), exact: true }));
    await page.keyboard.press("Delete");
  } else await page.keyboard.press("ControlOrMeta+z");
  // What a slow machine would still be dispatching.
  await page.waitForTimeout(150);
}

/** Sets up the job for `change`, opens it, and leaves the editor ready to press `action` with the wall of `t` in the bar. */
async function ready(app: any, page: Page, mcp: any, action: Action, change: Change, name: string) {
  let id: string;
  let t: Target;
  if (change === "offer") {
    id = await olderOffer(mcp, name);
  } else {
    id = await createExample("gable", name);
  }
  await app.open({ prefs: { followAi: false } });
  await app.openJob(name);
  if (change === "offer") await expect(recalcToast(app)).toHaveCount(1);
  await app.saved();
  if (change === "add_part") {
    await mcp.call("add_part", { id, part: { name: "Garáž", points: [[20, 0], [26, 0], [26, 6], [20, 6]] }, summary: "Garáž přidána" });
    await waitForProject(id, (p) => p.parts.length === 2);
    await expect(page.locator(".statusbar")).toContainText("budovy: 2");
    t = { wall: 4, part: 1, edge: 0, same: "edge.pitch", other: "edge.overhang" };
  } else if (change === "overhang") {
    await mcp.call("update_edges", { id, changes: [{ index: 0, overhang: 0.9 }], summary: "Přesah podle řezu" });
    await waitForProject(id, (p) => p.parts[0].edges[0].overhang === 0.9);
    t = { wall: 0, part: 0, edge: 0, same: "edge.overhang", other: "edge.pitch" };
  } else if (change === "notes") {
    await mcp.call("update_job", { id, notes: "Od AI", summary: "Poznámka od AI" });
    await waitForProject(id, (p) => p.notes === "Od AI");
    t = { wall: 0, part: 0, edge: 0, same: "notes", other: "edge.pitch" };
  } else {
    t = { wall: 0, part: 0, edge: 0, same: "edge.pitch", other: "edge.overhang" };
  }
  await app.saved();
  await selectWall(page, t.wall, t.wall < 4 ? t.wall + 1 : t.wall - 3);
  await expect(field(page, "edge.pitch")).toBeVisible();
  // Where each action's button is.
  if (action === "revert" || action === "put-back-ai") await dockTab(page, "dock.ai").click();
  else if (action === "put-back-card") await dockTab(page, "dock.results").click();
  else await dockTab(page, "dock.job").click();
  const button =
    action === "revert" ? entries(page).first().getByRole("button", { name: cs("ai.revert") })
    : action === "revert-all" ? page.locator(".review").getByRole("button", { name: cs("ai.revert") })
    : action === "put-back-ai" ? putBack(page)
    : page.getByTestId("snap-restore");
  await expect(button).toHaveCount(1);
  return { id, t, button };
}

/** The job the action must leave, whatever was tried meanwhile. */
function reverted(change: Change, before: any) {
  return (p: any) => {
    if (change === "add_part") return p.parts.length === 1;
    if (change === "overhang") return p.parts[0].edges[0].overhang === before.parts[0].edges[0].overhang;
    if (change === "notes") return !p.notes;
    return p.result_snapshot?.engine_version === "0.8.0" && p.result_snapshot.roof_area === 150;
  };
}

const MATRIX: [Action, Change][] = [
  ["revert", "add_part"],
  ["revert", "overhang"],
  ["revert", "notes"],
  ["revert-all", "add_part"],
  ["revert-all", "overhang"],
  ["revert-all", "notes"],
  ["put-back-ai", "offer"],
  ["put-back-card", "offer"],
];
const INTRUDERS: Intruder[] = ["same-enter", "other-enter", "typed-no-enter", "delete", "undo"];

test.describe("round 11: Revert and the put-back are a transaction; an edit tried meanwhile is refused (design A1)", () => {
  test.skip(process.platform === "win32", "the lock holder uses Python's fcntl");

  for (const [action, change] of MATRIX) {
    for (const intruder of INTRUDERS) {
      test(`${action} of ${change}, ${intruder} while the call is out: refused, file = screen, the value lands after (HUNT-1001, UX-1001, UX-1002, ERR-1004)`, async ({ app, page, mcp }) => {
        const name = `A1 ${action} ${change} ${intruder}`;
        const { id, t, button } = await ready(app, page, mcp, action, change, name);
        const before = await loadProject(id);
        // The original (the base example's) state of the AI's change: what the file must hold after.
        const base = change === "overhang" ? { ...before, parts: [{ ...before.parts[0], edges: [{ ...before.parts[0].edges[0], overhang: 0.5 }] }] } : before;
        const call = timeline(page, CMD[action]);
        const done = watchSaved(page, id, t);
        // Held until the intruder was tried (never for a fixed time, TEST-706, TEST-1004).
        const lock = await holdLock(id, 60_000);
        try {
          await button.click();
          await expect.poll(() => call.sent.length, { timeout: 4000 }).toBe(1);
          // The put-back has its own word (UX-1102).
          const word = cs(action.startsWith("put-back") ? "editor.restoringOffer" : "editor.reverting");
          await expect(status(page)).toHaveText(word);
          await intrude(page, app, intruder, t);
          expect(call.answered, "the call is still out while the intruder was tried").toHaveLength(0);
          // Refused: the editor still says it is reverting, and nothing was saved.
          await expect(status(page)).toHaveText(word);
        } finally {
          lock.kill();
        }
        const after = await waitForProject(id, reverted(change, base), 8000);
        await expect.poll(() => call.answered.length).toBe(1);
        await app.saved();
        // Sampled on past the answer: a later save or reload would show here.
        await page.waitForTimeout(1500);
        const { bad, samples } = await done();
        expect(samples, "the status was sampled").toBeGreaterThan(MIN_SAMPLES);
        expect(bad, "the status said Uloženo while the screen and the file differed").toEqual([]);
        // Nothing of the intruder in the file, and the file is still the core's answer.
        const now = await loadProject(id);
        expect(now).toEqual(after);
        if (change !== "add_part") expect(now.parts).toHaveLength(1);
        for (const p of now.parts) for (const e of p.edges) expect(e.pitch_deg).not.toBe(33);
        expect(now.parts[0].edges[0].overhang).not.toBe(0.7);
        expect(now.notes ?? "").not.toBe(VALUE.notes);
        const shown = await states(page, id, t);
        expect(shown.screen).toEqual(shown.file);
        expect(app.toasts().filter({ hasText: cs("editor.actionStale") })).toHaveCount(0);

        // The value typed now lands within 3 s.
        if (intruder === "delete" || intruder === "undo") return;
        const which = intruder === "other-enter" ? t.other : intruder === "typed-no-enter" && t.same === "notes" ? t.other : t.same;
        if (which === "notes") {
          await dockTab(page, "dock.job").click();
          await notesField(page).fill(VALUE.notes);
          await notesField(page).press("Tab");
          await waitForProject(id, (p) => p.notes === VALUE.notes, 3000);
          return;
        }
        if (!(await field(page, which).count())) await selectWall(page, 0);
        const target = (await loadProject(id)).parts.length > t.part ? t : { ...t, part: 0, edge: 0 };
        await field(page, which).fill(VALUE[which]);
        await field(page, which).press("Enter");
        const key = which === "edge.pitch" ? "pitch_deg" : "overhang";
        await waitForProject(id, (p) => p.parts[target.part]?.edges[target.edge]?.[key] === NUMBER[which], 3000);
        await app.saved();
        const end = await states(page, id, target);
        expect(end.screen).toEqual(end.file);
      });
    }
  }

  test("a value typed before the press is stored and saved before the call (commitFocused + save, design A1)", async ({ app, page, mcp }) => {
    const { id, t, button } = await ready(app, page, mcp, "revert", "notes", "A1 typed before");
    void t;
    await field(page, "edge.pitch").click();
    await field(page, "edge.pitch").fill("33");
    // No Enter: the press stores it.
    await button.click();
    const p = await waitForProject(id, (p) => !p.notes && p.parts[0].edges[0].pitch_deg === 33, 8000);
    expect(p.parts[0].edges[0].pitch_deg).toBe(33);
    await app.saved();
    await expect(field(page, "edge.pitch")).toHaveValue("33");
  });

  test("a busy answer (the job's lock was held too long) changes nothing, says so and keeps the button (design A1)", async ({ app, page, mcp }) => {
    const { id, button } = await ready(app, page, mcp, "revert", "notes", "A1 busy");
    // The core's answer when the lock was held longer than its wait (the other half of design A1).
    await page.route("**/revert_last_change", (route) => route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ err: { code: "busy", id, detail: "the job is locked by another writer" } }) }));
    await button.click();
    const note = app.toasts().filter({ hasText: cs("err.busy") });
    await expect(note).toHaveCount(1);
    await expect(note).toContainText("busy");
    await app.saved();
    expect((await loadProject(id)).notes).toBe("Od AI");
    await expect(entries(page).first().getByRole("button", { name: cs("ai.revert") })).toHaveCount(1);
    // The editor takes edits again.
    await expect(field(page, "edge.pitch")).toBeEditable();
    // notifyError logs the answer; expected here.
    app.errors.length = 0;
  });
});

test.describe("round 11: the result card's buttons keep the keyboard's focus (UI-1001)", () => {
  test("the card's put-back pressed from the keyboard leaves the focus on the card", async ({ app, page, mcp }) => {
    const id = await olderOffer(mcp, "Karta klávesnice");
    await app.open({ prefs: { followAi: false } });
    await app.openJob("Karta klávesnice");
    await expect(recalcToast(app)).toHaveCount(1);
    await app.saved();
    await dockTab(page, "dock.results").click();
    await page.getByTestId("snap-restore").focus();
    await page.keyboard.press("Enter");
    await waitForProject(id, (p) => p.result_snapshot?.engine_version === "0.8.0");
    await expect(page.getByTestId("snap-restore")).toHaveCount(0);
    await expect(page.getByTestId("snap-title")).toBeFocused();
    expect(await page.evaluate(() => document.activeElement === document.body)).toBe(false);
  });

  test("the card's Store on a job without a stored result, pressed from the keyboard, leaves the focus on the card", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "Karta uložit");
    await mcp.call("update_job", { id, status: "offer", summary: "Nabídka" });
    const p = await loadProject(id);
    delete p.result_snapshot;
    await bridge("save_project", { id, project: p });
    await acceptAll(id);
    await app.open({ prefs: { followAi: false } });
    await app.openJob("Karta uložit");
    await app.saved();
    await dockTab(page, "dock.results").click();
    await page.getByTestId("snap-store").focus();
    await page.keyboard.press("Enter");
    await waitForProject(id, (p) => !!p.result_snapshot);
    await expect(page.getByTestId("snap-store")).toHaveCount(0);
    await expect(page.getByTestId("snap-title")).toBeFocused();
  });
});

test.describe("round 11: a job's notices stay in its editor (UI-1002, UI-1003, TEST-1005)", () => {
  test("the recalculation notice names the job and closes with its editor; the next job's editor never shows it (UI-1002)", async ({ app, page, mcp }) => {
    await olderOffer(mcp, "Sedlová nabídka");
    await createExample("hip", "Jiná zakázka");
    await app.open({ prefs: { followAi: false } });
    await app.openJob("Sedlová nabídka");
    await expect(recalcToast(app)).toHaveCount(1);
    await expect(recalcToast(app)).toContainText(cs("snap.titleOf", { name: "Sedlová nabídka" }));
    await page.getByRole("button", { name: cs("editor.back") }).first().click();
    await expect(recalcToast(app)).toHaveCount(0, { timeout: 1500 });
    await app.openJob("Jiná zakázka");
    await noToast(recalcToast(app), 1500, "the other job's notice");
  });

  test("a job name without spaces wraps in the toast's title instead of running under its close button (UI-1003)", async ({ app, page, mcp }) => {
    const name = "Novak_Horni_Becva_1234_garaz_a_zimni_zahrada_varianta_B";
    // A stored result with an impossible time: the load warning's title names the job (UX-901).
    const id = await createExample("gable", name);
    const file = path.join(LIBRARY_DIR, `${id}.tectora`);
    const p = JSON.parse(fs.readFileSync(file, "utf8"));
    p.result_snapshot = { engine_version: "0.9.0", taken_ms: 4242424242, roof_area: 99, ridge_height: 5, lines_by_code: { T: 20 } };
    fs.writeFileSync(file, JSON.stringify(p).replace("4242424242", "8640000000000001"));
    void mcp;
    await app.open({ prefs: { followAi: false } });
    await app.openJob(name);
    const toast = app.toasts().filter({ hasText: cs("snap.titleOf", { name }) });
    await expect(toast).toHaveCount(1);
    const fit = await toast.evaluate((el) => {
      const strong = el.querySelector("strong")!;
      const close = el.querySelector(".close")!.getBoundingClientRect();
      const range = document.createRange();
      range.selectNodeContents(strong);
      const text = range.getBoundingClientRect();
      return { over: strong.scrollWidth - strong.clientWidth, wide: el.scrollWidth - el.clientWidth, textRight: text.right, closeLeft: close.left };
    });
    expect(fit.over, "the title is wider than its box").toBeLessThanOrEqual(0);
    expect(fit.wide, "the toast's content is wider than the toast").toBeLessThanOrEqual(0);
    expect(fit.textRight, "the title runs under the close button").toBeLessThanOrEqual(fit.closeLeft + 1);
  });

  test("a missed notice is not shown once a new result was stored (TEST-1005)", async ({ app, page, mcp }) => {
    const id = await olderOffer(mcp, "Zmeškané");
    // Left before the notice could be shown (as ERR-902): the next open would show it.
    let open!: () => void;
    const gate = new Promise<void>((r) => (open = r));
    let held = 0;
    await page.route("**/refresh_stored_result", async (route) => {
      const res = await route.fetch();
      held++;
      await gate;
      await route.fulfill({ response: res }).catch(() => {});
    });
    await app.open({ prefs: { followAi: false } });
    await page.getByRole("row").filter({ hasText: "Zmeškané" }).first().dblclick();
    await expect.poll(() => held).toBe(1);
    await page.getByRole("button", { name: cs("editor.back") }).first().click();
    const answered = page.waitForResponse((r) => r.url().endsWith("/refresh_stored_result"));
    open();
    await answered;
    await page.unroute("**/refresh_stored_result");
    // A new result stored meanwhile (an AI, accepted): the stored result is no recalculation any more.
    await mcp.call("store_result", { id, summary: "Výsledek uložen" });
    await acceptAll(id);
    await page.waitForTimeout(300);
    await app.openJob("Zmeškané");
    await app.saved();
    await noToast(recalcToast(app), 1500, "the missed notice of a recalculation that is no longer the stored result");
  });
});

test.describe("round 11: a recalculated offer sent as a bundle (UX-1003)", () => {
  test("the sender is told the offer's figures stay here, and the receiver's card says recalculated, without the put-back", async ({ app, page, mcp }) => {
    const id = await olderOffer(mcp, "Odeslaná nabídka");
    await app.open({ prefs: { followAi: false } });
    await app.openJob("Odeslaná nabídka");
    await expect(recalcToast(app)).toHaveCount(1);
    await app.saved();
    const engine = (await loadProject(id)).result_snapshot.engine_version as string;
    // Export from the job tab.
    fs.mkdirSync(FILES_DIR, { recursive: true });
    const file = path.join(FILES_DIR, "odeslana.tectorajob");
    await dockTab(page, "dock.job").click();
    await app.answerFileDialog(file);
    await page.getByRole("button", { name: cs("files.export") }).click();
    // A warning naming the job and the way (UX-1101).
    await expect(app.toasts().filter({ hasText: cs("snap.exportRecalculated", { tab: cs("dock.results"), button: cs("snap.revertRefresh") }) })).toHaveCount(1);
    await expect.poll(() => fs.existsSync(file)).toBe(true);

    // Received (the same library stands in for the colleague's).
    const got = await bridge<string>("import_job", { path: file });
    await page.getByRole("button", { name: cs("editor.back") }).first().click();
    // Renamed on disk only to tell the two rows apart (the metadata is no roof input).
    const received = await loadProject(got);
    received.name = "Přijatá nabídka";
    await bridge("save_project", { id: got, project: received });
    await acceptAll(got);
    await app.openJob("Přijatá nabídka");
    await app.saved();
    await dockTab(page, "dock.results").click();
    const state = page.getByTestId("snap-state");
    await expect(state).toContainText(cs("snap.recalculated", { date: "§", from: "0.8.0", to: engine }).split("§")[0].trim());
    await expect(state).toContainText(`(Tectora 0.8.0 → ${engine})`);
    await expect(state).not.toContainText(cs("snap.same"));
    await expect(page.getByTestId("snap-received")).toContainText("150,00 m²");
    await expect(page.getByTestId("snap-restore")).toHaveCount(0);
    await dockTab(page, "dock.ai").click();
    await expect(putBack(page)).toHaveCount(0);
  });
});
