// Guards for the round-6 audit (2026-11-22) findings in the editor: the drag
// pause of the edit rule (app/src/lib/jobedits.ts) and what runs during a
// drag, Undo with an edit still with the core, the saving state, the focus
// of a dialog's OK, and a job at the limits in the plan.
//
// ERR-501 a second finger during a touch drag; ERR-502 keys during a drag;
// UI-502 a corner drag holds the pause too; UX-501 Undo waits for the edit
// just typed; UX-502 "saving" while an edit is with the core; UI-503 the
// focus stays in a dialog whose OK waits; UI-501 / TEST-501 200 buildings.
import type { Page, Route } from "@playwright/test";
import { bar, clickPlan, openEditor, plan, planXY, selectWall, setField, tool } from "./editor";
import { bridge, createExample, expect, loadProject, test, waitForProject, type App } from "./fixtures";
import { cs } from "./i18n";

const dormerWidth = (d: any) => +Math.hypot(d.end.x - d.edges[0].start.x, d.end.y - d.edges[0].start.y).toFixed(3);
const square = (x: number, y: number, s: number) => [
  { x, y },
  { x: x + s, y },
  { x: x + s, y: y + s },
  { x, y: y + s },
];
const extentX = (o: any) => {
  const v = o.corners.map((c: any) => c.x) as number[];
  return +(v.reduce((a, b) => Math.max(a, b)) - v.reduce((a, b) => Math.min(a, b))).toFixed(3);
};

async function acceptAll(id: string) {
  const log = await bridge<{ ts_ms: number }[]>("project_activity", { id });
  if (log.length) await bridge("accept_changes", { id, tsMs: log.reduce((m, a) => Math.max(m, a.ts_ms), 0) });
}

async function threeDormers(mcp: any, id: string) {
  for (const x of [1.5, 5, 8.5]) await mcp.call("add_dormer", { id, kind: "gable", start: [x, 1.6], width: 1.5, summary: "Dormer." });
  await acceptAll(id);
}

async function openAfterAi(app: App, name: string) {
  await app.open();
  const editor = app.page.locator(".editor .statusbar");
  if (!(await editor.isVisible().catch(() => false))) await app.page.getByRole("row").filter({ hasText: name }).first().dblclick({ timeout: 3000 }).catch(() => {});
  await expect(editor).toBeVisible();
  await expect(plan(app.page)).toBeVisible();
}

async function withItems(id: string, edit: (p: any) => void) {
  const p = await loadProject(id);
  edit(p);
  await bridge("save_project", { id, project: p });
}

/** The centre of dormer `i` (0-based) in the plan, in page pixels. */
async function dormerAt(page: Page, i: number) {
  const b = (await page.locator("g.dormer").nth(i).locator("path.dormer-area").first().boundingBox())!;
  return { x: b.x + b.width / 2, y: b.y + b.height / 2 };
}

async function selectDormer(page: Page, n: number) {
  const db = bar(page, "dormer.title");
  const at = await dormerAt(page, n - 1);
  await page.mouse.click(at.x, at.y);
  await expect(db.locator(".which strong")).toHaveText(cs("dormer.n", { n }));
  return db;
}

/** Holds the bridge's answers to `cmd` by `ms` while `on()` is true. */
async function slow(page: Page, cmd: string, ms: number, on: () => boolean = () => true) {
  await page.route(new RegExp(`/${cmd}$`), async (r: Route) => {
    if (on()) await new Promise((res) => setTimeout(res, ms));
    await r.continue();
  });
}

/**
 * Holds the first answer to `cmd` until `gate()` is true, then `ms` more
 * (`times.answered` is when it went back); every later one `laterMs`.
 */
async function holdUntil(page: Page, cmd: string, gate: () => boolean, ms: number, times: { answered?: number }, laterMs = 0) {
  let first = true;
  await page.route(new RegExp(`/${cmd}$`), async (r: Route) => {
    if (!first) {
      await new Promise((res) => setTimeout(res, laterMs));
      return r.continue();
    }
    first = false;
    while (!gate()) await new Promise((res) => setTimeout(res, 20));
    await new Promise((res) => setTimeout(res, ms));
    const resp = await r.fetch();
    times.answered = Date.now();
    await r.fulfill({ response: resp });
  });
}

/** Every toast seen from now on, as `kind: text`. */
async function watchToasts(page: Page): Promise<() => Promise<string[]>> {
  await page.evaluate(() => {
    const w = window as any;
    w.__toasts = [];
    const seen = new WeakSet<Element>();
    const scan = () =>
      document.querySelectorAll(".stack .toast").forEach((t) => {
        if (seen.has(t)) return;
        seen.add(t);
        const kind = ["info", "warning", "error"].find((k) => t.classList.contains(k)) ?? "?";
        w.__toasts.push(`${kind}: ${(t.textContent ?? "").replace(/\s+/g, " ").trim()}`);
      });
    scan();
    new MutationObserver(scan).observe(document.body, { childList: true, subtree: true });
  });
  return () => page.evaluate(() => (window as any).__toasts as string[]);
}

/** The longest time the page's main thread did not run a timer while `act` ran, ms. */
async function gapDuring(page: Page, act: () => Promise<void>): Promise<number> {
  await page.evaluate(() => {
    const w = window as any;
    w.__gap = { worst: 0, last: performance.now(), on: true };
    const tickOnce = () => {
      const g = w.__gap;
      const now = performance.now();
      g.worst = Math.max(g.worst, now - g.last);
      g.last = now;
      if (g.on) setTimeout(tickOnce, 20);
    };
    setTimeout(tickOnce, 20);
  });
  await act();
  return page.evaluate(() => {
    const g = (window as any).__gap;
    g.on = false;
    return Math.round(Math.max(g.worst, performance.now() - g.last));
  });
}

/** Types like a person: click, select all, type. */
async function typeInto(page: Page, field: ReturnType<Page["locator"]>, text: string) {
  await field.click();
  await field.press("ControlOrMeta+a");
  await page.keyboard.type(text);
}

/** Presses at `at` and drags by `dy` px in `n` steps, the pointer kept down. */
async function dragDown(page: Page, at: { x: number; y: number }, dy: number, n = 6) {
  await page.mouse.move(at.x, at.y);
  await page.mouse.down();
  for (let i = 1; i <= n; i++) {
    await page.mouse.move(at.x, at.y + (dy * i) / n);
    await page.waitForTimeout(30);
  }
}

// With TECTORA_SHOTS=<folder>/ and TECTORA_SHOTS_TAG=before|after every test
// leaves a screenshot `r6-<finding>-<tag>.png` there; without it nothing is written.
test.afterEach(async ({ page }, info) => {
  const dir = process.env.TECTORA_SHOTS;
  if (!dir) return;
  const id = info.titlePath.join(" ").match(/\(([A-Z]+-\d+)/)?.[1] ?? "misc";
  const n = info.title.replace(/[^a-z0-9]+/gi, "-").slice(0, 24);
  await page.screenshot({ path: `${dir}r6-${id}-${n}-${process.env.TECTORA_SHOTS_TAG ?? "after"}.png`, timeout: 10_000 }).catch(() => {});
});

test.describe("round-6 audit: a second finger during a touch drag (ERR-501)", () => {
  test.use({ hasTouch: true });

  test("a second touch on another dormer does not take over the drag: the drag is saved, a typed width lands and Back leaves", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "Dva prsty");
    await threeDormers(mcp, id);
    await openAfterAi(app, "Dva prsty");
    const cdp = await page.context().newCDPSession(page);
    const touch = (type: string, pts: { x: number; y: number; id: number }[]) => cdp.send("Input.dispatchTouchEvent", { type, touchPoints: pts });
    const p3 = await dormerAt(page, 2);
    const p1 = await dormerAt(page, 0);
    await touch("touchStart", [{ ...p3, id: 1 }]);
    for (let i = 1; i <= 10; i++) {
      await touch("touchMove", [{ x: p3.x, y: p3.y + i * 3, id: 1 }]);
      await page.waitForTimeout(30);
    }
    // Finger 2 lands on dormer 1 while finger 1 still holds dormer 3.
    await touch("touchStart", [{ x: p3.x, y: p3.y + 30, id: 1 }, { ...p1, id: 2 }]);
    await page.waitForTimeout(100);
    await touch("touchMove", [{ x: p3.x, y: p3.y + 30, id: 1 }, { x: p1.x + 20, y: p1.y + 20, id: 2 }]);
    await touch("touchEnd", []);
    const moved = await waitForProject(id, (p) => p.dormers[2].edges[0].start.y !== 1.6, 6000);
    // Dormer 1 was not dragged by the second finger.
    expect(moved.dormers[0].edges[0].start).toEqual({ x: 1.5, y: 1.6 });
    // A width checked by the core lands at once: the drag pause has ended.
    const db = await selectDormer(page, 2);
    await typeInto(page, db.getByLabel(cs("dormer.width")), "2");
    await page.keyboard.press("Enter");
    await waitForProject(id, (p) => dormerWidth(p.dormers[1]) === 2, 3000);
    await page.locator("header .icon-btn").first().click();
    await expect(page.locator(".editor")).toBeHidden({ timeout: 5000 });
  });

  test("leaving the job while the finger still holds a dormer ends the drag where it is: the editor closes and the drag is saved (TEST-601)", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "Odchod s prstem");
    await threeDormers(mcp, id);
    await openAfterAi(app, "Odchod s prstem");
    const cdp = await page.context().newCDPSession(page);
    const touch = (type: string, pts: { x: number; y: number; id: number }[]) => cdp.send("Input.dispatchTouchEvent", { type, touchPoints: pts });
    const p3 = await dormerAt(page, 2);
    await touch("touchStart", [{ ...p3, id: 1 }]);
    for (let i = 1; i <= 10; i++) {
      await touch("touchMove", [{ x: p3.x, y: p3.y + i * 3, id: 1 }]);
      await page.waitForTimeout(30);
    }
    // The finger is still down. Back is closed for a pointer during a drag
    // (jobedits.ts rule 7); a leave from elsewhere (a file opened from the
    // system, the follow offer) runs the same guard as this click.
    await page.locator("header .icon-btn").first().dispatchEvent("click");
    await expect(page.locator(".editor")).toBeHidden({ timeout: 5000 });
    const moved = await waitForProject(id, (p) => p.dormers[2].edges[0].start.y !== 1.6, 3000);
    expect(moved.dormers.length).toBe(3);
    await touch("touchEnd", []);
  });
});

test.describe("round-6 audit: keys during a drag (ERR-502)", () => {
  test("Delete and Ctrl+Z pressed while a dormer is dragged do nothing (also not after the drop); after it the keys work on the dropped job", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "Klávesy při tahu");
    await threeDormers(mcp, id);
    await openAfterAi(app, "Klávesy při tahu");
    // A step for Ctrl+Z to undo: on a job opened fresh the press proved nothing (TEST-601 G3b).
    const d1 = await selectDormer(page, 1);
    await typeInto(page, d1.getByLabel(cs("dormer.width")), "2");
    await page.keyboard.press("Enter");
    await waitForProject(id, (p) => dormerWidth(p.dormers[0]) === 2);
    await app.saved();
    await selectDormer(page, 3);
    // Past the double-press time: a second press within it opens the dormer window.
    await page.waitForTimeout(600);
    await dragDown(page, await dormerAt(page, 2), 30);
    await page.keyboard.press("Delete");
    await page.keyboard.press("ControlOrMeta+z");
    await page.waitForTimeout(300);
    // Nothing was asked and nothing removed mid-drag.
    expect(await app.dialogs()).toEqual([]);
    await expect(page.locator("g.dormer")).toHaveCount(3);
    await page.mouse.move((await dormerAt(page, 2)).x, (await dormerAt(page, 2)).y + 10);
    await page.mouse.up();
    const dropped = await waitForProject(id, (p) => p.dormers.length === 3 && p.dormers[2].edges[0].start.y !== 1.6, 6000);
    // The Ctrl+Z pressed mid-drag undid nothing, then or after the drop (refused, not replayed).
    await page.waitForTimeout(800);
    await app.saved();
    expect(dormerWidth((await loadProject(id)).dormers[0])).toBe(2);
    expect(await app.dialogs()).toEqual([]);
    // After the drop the keys work: Delete (Yes) removes the dragged dormer for good.
    await app.answerConfirm(true);
    await page.keyboard.press("Delete");
    await waitForProject(id, (p) => p.dormers.length === 2, 6000);
    await page.waitForTimeout(800);
    await app.saved();
    const q = await loadProject(id);
    expect(q.dormers.length).toBe(2);
    expect(q.dormers.map((d: any) => d.edges[0].start.x)).toEqual([dropped.dormers[0].edges[0].start.x, dropped.dormers[1].edges[0].start.x]);
  });

  test("Escape during a drag puts the dormer back where it was and saves nothing", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "Esc při tahu");
    await threeDormers(mcp, id);
    await openAfterAi(app, "Esc při tahu");
    const before = await loadProject(id);
    await selectDormer(page, 3);
    await page.waitForTimeout(600);
    const at = await dormerAt(page, 2);
    await dragDown(page, at, 30);
    await page.keyboard.press("Escape");
    // Further moves of the same drag do nothing.
    await page.mouse.move(at.x, at.y + 50);
    await page.mouse.up();
    await page.waitForTimeout(1500);
    await app.saved();
    expect((await loadProject(id)).dormers).toEqual(before.dormers);
    const back = await dormerAt(page, 2);
    expect(Math.abs(back.y - at.y)).toBeLessThan(3);
  });
});

test.describe("round-6 audit: no autosave during a drag (TEST-601 G11)", () => {
  test("a save still due when the next drag begins waits for its drop: nothing is written while the dormer is held", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "Uložení při tahu");
    await threeDormers(mcp, id);
    await openAfterAi(app, "Uložení při tahu");
    const saves: number[] = [];
    page.on("request", (r) => {
      if (/\/save_project(_checked)?$/.test(r.url())) saves.push(Date.now());
    });
    await selectDormer(page, 3);
    await page.waitForTimeout(600);
    const p3 = await dormerAt(page, 2);
    const p1 = await dormerAt(page, 0);
    // Drag 1 drops dormer 3: its save is due 500 ms later.
    await dragDown(page, p3, 30);
    // Drag 2 starts at once on dormer 1 (the three inputs sent in one go).
    await Promise.all([page.mouse.up(), page.mouse.move(p1.x, p1.y), page.mouse.down()]);
    const down = Date.now();
    await page.waitForTimeout(150);
    // The first drop is not saved yet: its save is the one that must wait.
    expect(((await page.locator(".statusbar .save").textContent()) ?? "").trim(), "the save of drag 1 is still due when drag 2 begins").not.toBe(cs("editor.saved"));
    for (let i = 1; i <= 10; i++) {
      await page.mouse.move(p1.x, p1.y + i * 3);
      await page.waitForTimeout(30);
    }
    // Held for 3 s, six times the autosave delay.
    await page.waitForTimeout(3000);
    const mid = await loadProject(id);
    const up = Date.now();
    expect(saves.filter((t) => t > down + 100 && t < up), "saves while dormer 1 was held").toEqual([]);
    // The file has neither the half-dragged dormer 1 nor, yet, drag 1.
    expect(mid.dormers[0].edges[0].start).toEqual({ x: 1.5, y: 1.6 });
    await page.mouse.up();
    const end = await waitForProject(id, (p) => p.dormers[0].edges[0].start.y !== 1.6 && p.dormers[2].edges[0].start.y !== 1.6, 6000);
    expect(end.dormers.length).toBe(3);
    await app.saved();
  });
});

test.describe("round-6 audit: a corner drag holds the edits too (UI-502)", () => {
  test("a building height answered while a corner is dragged is kept, with the corner moved and no refusal", async ({ app, page }) => {
    const id = await createExample("gable", "Tah rohu");
    await openEditor(app, "Tah rohu");
    const before = await loadProject(id);
    const c1 = before.parts[0].edges[1].start;
    await selectWall(page, 0);
    const toasts = await watchToasts(page);
    const times: { answered?: number; down?: number; up?: number } = {};
    // The first answer comes once the corner is held, every later one 400 ms
    // after it is asked: slower than the corner's steps, as on a big job.
    // Without the pause the edit is asked four times and refused; all four
    // answers must land while the corner is still held, every run (TEST-601
    // G5): the first 600 ms after the press, the later ones 150 ms each,
    // and the drag lasts well over 4 s.
    const asked: number[] = [];
    page.on("request", (r) => {
      if (/\/set_part_height$/.test(r.url())) asked.push(Date.now());
    });
    await holdUntil(page, "set_part_height", () => times.down !== undefined, 600, times, 150);
    await typeInto(page, bar(page, "def.title").getByTestId("building-height"), "4");
    await page.keyboard.press("Enter");
    await page.waitForTimeout(200);
    const at = await planXY(page, c1.x, c1.y);
    await page.mouse.move(at.x, at.y);
    await page.mouse.down();
    times.down = Date.now();
    // Steps for about 4 s, each far enough to move the corner (past the 5 cm
    // snap and the alignment magnet): every step changes the job.
    for (let i = 1; i <= 60; i++) {
      await page.mouse.move(at.x + 20 + i * 3, at.y + 20 + i * 2);
      await page.waitForTimeout(70);
    }
    await page.mouse.up();
    times.up = Date.now();
    expect(times.answered, "the core answered while the corner was held").toBeGreaterThan(times.down);
    // With 2.5 s to spare: four asks of 150 ms each would all have come back before the release.
    expect(times.answered!).toBeLessThan(times.up - 2500);
    // The answer that came during the drag was not applied and not asked
    // again at every step: one ask before the release, one after it.
    expect(asked.filter((t) => t < times.up!).length, "asks while the corner was held").toBe(1);
    const p = await waitForProject(id, (p) => p.parts[0].edges[1].start.x !== c1.x && (p.parts[0].height ?? p.parts[0].edges[0].eave_height) === 4, 10_000);
    expect(p.parts[0].edges[1].start.x).toBeGreaterThan(c1.x);
    expect(p.parts[0].edges[1].start.y).not.toBe(c1.y);
    expect((await toasts()).filter((t) => !t.startsWith("info"))).toEqual([]);
  });
});

test.describe("round-6 audit: Undo and the saving state with an edit at the core", () => {
  test("Undo pressed while a typed width is with the core undoes that width, and Redo brings it back (UX-501)", async ({ app, page }) => {
    const id = await createExample("gable", "Zpět a jádro");
    await withItems(id, (p) => (p.chimneys = [{ corners: square(5, 1.5, 0.6), height: 8 }]));
    await openEditor(app, "Zpět a jádro");
    const bb = (await page.locator("path.opening.chimney").first().boundingBox())!;
    await page.mouse.click(bb.x + bb.width / 2, bb.y + bb.height / 2);
    const ob = bar(page, "opening.chimney");
    const width = ob.getByLabel(cs("opening.width"));
    await typeInto(page, width, "0,9");
    await page.keyboard.press("Enter");
    await waitForProject(id, (p) => extentX(p.chimneys[0]) === 0.9);
    await slow(page, "item_geometry", 1500);
    await typeInto(page, width, "1,2");
    await page.keyboard.press("Enter");
    await page.waitForTimeout(200);
    await tool(page, "editor.undo").click();
    await page.waitForTimeout(2500);
    await app.saved();
    expect(extentX((await loadProject(id)).chimneys[0])).toBe(0.9);
    await expect(tool(page, "editor.redo")).toBeEnabled();
    await tool(page, "editor.redo").click();
    await waitForProject(id, (p) => extentX(p.chimneys[0]) === 1.2);
  });

  test("the status bar says saving while a typed building height is with the core (UX-502)", async ({ app, page }) => {
    const id = await createExample("gable", "Ukládám");
    await openEditor(app, "Ukládám");
    await selectWall(page, 0);
    await app.saved();
    await slow(page, "set_part_height", 2500);
    await typeInto(page, bar(page, "def.title").getByTestId("building-height"), "4,2");
    await page.keyboard.press("Enter");
    // Read as it is, no retry: a retrying check waited for the real save
    // that follows the answer and passed with "saved" shown all along
    // (TEST-601 G7). Sampled while the edit is with the core (held 2.5 s).
    await page.waitForTimeout(500);
    const seen: string[] = [];
    for (let i = 0; i < 15; i++) {
      seen.push(((await page.locator(".statusbar .save").textContent()) ?? "").trim());
      await page.waitForTimeout(100);
    }
    expect(seen.filter((s) => s !== cs("editor.saving")), "status while the edit is with the core").toEqual([]);
    expect((await loadProject(id)).parts[0].edges[0].eave_height).not.toBe(4.2);
    await waitForProject(id, (p) => (p.parts[0].height ?? p.parts[0].edges[0].eave_height) === 4.2, 8000);
    await app.saved();
  });
});

test.describe("round-6 audit: dialogs keep the focus (UI-503)", () => {
  test("Enter on OK of the calculation helper keeps the focus on OK while it waits and after a refusal", async ({ app, page }) => {
    await createExample("gable", "Fokus pomocníka");
    await openEditor(app, "Fokus pomocníka");
    await selectWall(page, 0);
    await page.route(/\/roof_helper_apply$/, async (r: Route) => {
      await new Promise((res) => setTimeout(res, 1000));
      await r.fulfill({ contentType: "application/json", body: JSON.stringify({ err: { code: "refused", detail: "the helper gives values a wall does not take" } }) });
    });
    await bar(page, "def.title").getByRole("button", { name: cs("calc.button") }).click();
    const dlg = page.getByRole("dialog", { name: cs("calc.title") });
    await dlg.getByRole("tab", { name: cs("calc.buildup") }).click();
    const thick = dlg.getByRole("textbox", { name: cs("calc.thickness") });
    await thick.fill("0,3");
    await thick.press("Tab");
    const ok = dlg.getByRole("button", { name: cs("common.ok"), exact: true });
    // OK is enabled only once the helper's request is worked out: focused
    // before that, the focus stayed on the select the Tab went to and Enter
    // opened it (TEST-602).
    await expect(ok).toBeEnabled();
    await ok.focus();
    await expect(ok).toBeFocused();
    await page.keyboard.press("Enter");
    await page.waitForTimeout(300);
    await expect(ok).toHaveAttribute("aria-busy", "true");
    await expect(ok).toBeFocused();
    // A second Enter while it waits does not run it twice.
    await page.keyboard.press("Enter");
    await expect(app.toasts().filter({ hasText: cs("refuse.helperRange") })).toHaveCount(1);
    await expect(dlg).toBeVisible();
    await expect(ok).toBeFocused();
    await expect(ok).not.toHaveAttribute("aria-busy", "true");
  });
});

test.describe("round-6 audit: a job at the limits in the plan (UI-501, TEST-501)", () => {
  test("a job of 200 buildings opens in the editor and stays interactive", async ({ app, page }) => {
    test.setTimeout(300_000);
    const id = await createExample("gable", "Sídliště");
    await withItems(id, (p) => {
      const one = p.parts[0];
      p.dormers = [];
      p.chimneys = [];
      p.windows = [];
      p.parts = Array.from({ length: 200 }, (_, i) => ({
        ...one,
        name: `B${i + 1}`,
        edges: one.edges.map((e: any) => ({ ...e, start: { x: e.start.x + (i % 20) * 16, y: e.start.y + Math.floor(i / 20) * 14 } })),
      }));
    });
    await app.open();
    const t0 = Date.now();
    await page.getByRole("row").filter({ hasText: "Sídliště" }).first().dblclick();
    await expect(page.locator(".editor .statusbar")).toBeVisible({ timeout: 30_000 });
    await expect(page.locator(".results .hero strong")).toBeVisible({ timeout: 60_000 });
    const opened = Date.now() - t0;
    // The page answers at once, not after minutes of plan labels (UI-501).
    const longest = await page.evaluate(
      () =>
        new Promise<number>((done) => {
          let last = performance.now();
          let worst = 0;
          const end = last + 3000;
          const tickOnce = () => {
            const now = performance.now();
            worst = Math.max(worst, now - last);
            last = now;
            if (now < end) setTimeout(tickOnce, 20);
            else done(worst);
          };
          setTimeout(tickOnce, 20);
        }),
    );
    expect(opened, "opened in ms").toBeLessThan(60_000);
    expect(longest, "longest main-thread gap in ms after opening").toBeLessThan(2000);
    // Selecting a wall recolours the 3D view, it does not build it again
    // (UI-602: 4 to 5 s frozen per click in the split layout).
    const view3d = page.locator(".view .host").first();
    await expect(view3d).toHaveAttribute("data-builds", /\d+/, { timeout: 30_000 });
    const builds = await view3d.getAttribute("data-builds");
    const clickGap = await gapDuring(page, async () => {
      await selectWall(page, 0);
      await selectWall(page, 5, 2);
      await selectWall(page, 0);
    });
    expect(await view3d.getAttribute("data-builds"), "3D scene builds after three wall clicks").toBe(builds);
    expect(clickGap, "longest main-thread gap in ms while walls are selected").toBeLessThan(2000);
    // Zooming in to where labels show, and panning there, stays fluid
    // (UX-602: 1.5 to 2.3 s per step in the label placement).
    const box = (await plan(page).boundingBox())!;
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
    // Counted, not only timed: the tests the label pass runs per step
    // (`data-label-work`), which no load on the machine changes. Placing
    // every label against every wall, line and label was millions.
    const work: number[] = [];
    const zoomGap = await gapDuring(page, async () => {
      for (let i = 0; i < 6; i++) {
        await page.mouse.wheel(0, -260);
        await page.waitForTimeout(150);
        work.push(+((await plan(page).getAttribute("data-label-work")) ?? "NaN"));
      }
    });
    await expect(page.locator("g.face-tag").first()).toBeVisible();
    expect(await page.locator("text.edge-dim").count(), "wall lengths shown when zoomed in").toBeGreaterThan(0);
    const panGap = await gapDuring(page, async () => {
      await page.mouse.down({ button: "middle" });
      for (let i = 1; i <= 6; i++) {
        await page.mouse.move(box.x + box.width / 2 - i * 25, box.y + box.height / 2 - i * 15);
        await page.waitForTimeout(100);
      }
      await page.mouse.up({ button: "middle" });
    });
    work.push(+((await plan(page).getAttribute("data-label-work")) ?? "NaN"));
    expect(work.some((w) => w > 0), `label work per step ${work}`).toBe(true);
    for (const w of work) expect(w, `label work per step ${work}`).toBeLessThan(100_000);
    expect(Math.max(zoomGap, panGap), `longest main-thread gap in ms per zoom / pan step (zoom ${zoomGap}, pan ${panGap})`).toBeLessThan(2000);
    if (process.env.TECTORA_E2E_TIMES) console.log(`UI-602 / UX-602: click gap ${clickGap} ms, zoom gap ${zoomGap} ms, pan gap ${panGap} ms, label work ${work}`);
    // An edit is drawn again without freezing the page. The time to the file
    // is the debug core's calculation of 200 buildings, not the editor's:
    // what is checked is that the page keeps answering meanwhile (TEST-602).
    const pitch = bar(page, "def.title").getByLabel(cs("edge.pitch"));
    const editGap = await gapDuring(page, async () => {
      await typeInto(page, pitch, "38");
      await page.keyboard.press("Enter");
      await waitForProject(id, (p) => p.parts[0].edges[0].pitch_deg === 38, 90_000);
    });
    expect(editGap, "longest main-thread gap in ms while the edit is calculated and saved").toBeLessThan(2000);
    const ping = Date.now();
    await page.evaluate(() => 1);
    expect(Date.now() - ping, "the page answers right after the edit").toBeLessThan(2000);
    // The 3D view opens it too.
    await page.keyboard.press("F3");
    await expect(page.locator(".view canvas").first()).toBeVisible({ timeout: 30_000 });
  });
});

test.describe("round-7 audit: 500 hand-drawn faces of 500 corners reach the editor (TEST-607, class 16)", () => {
  test("the job opens in the plan and the 3D view and the page keeps answering", async ({ app, page }) => {
    test.setTimeout(300_000);
    const id = await createExample("gable", "Plochy na mezi");
    await withItems(id, (p) => {
      p.dormers = [];
      p.chimneys = [];
      p.windows = [];
      // 500 faces of 500 corners each, 8 m apart (the limits of check_limits).
      p.manual_faces = Array.from({ length: 500 }, (_, i) => ({
        pitch_deg: 30,
        points: Array.from({ length: 500 }, (_, k) => ({
          x: +(20 + (i % 25) * 8 + 3 * Math.cos((2 * Math.PI * k) / 500)).toFixed(3),
          y: +(Math.floor(i / 25) * 8 + 3 * Math.sin((2 * Math.PI * k) / 500)).toFixed(3),
        })),
      }));
    });
    expect((await loadProject(id)).manual_faces.length).toBe(500);
    await app.open();
    const t0 = Date.now();
    await page.getByRole("row").filter({ hasText: "Plochy na mezi" }).first().dblclick();
    await expect(page.locator(".editor .statusbar")).toBeVisible({ timeout: 60_000 });
    await expect(page.locator("path.manual-face")).toHaveCount(500, { timeout: 60_000 });
    expect(Date.now() - t0, "opened in ms").toBeLessThan(120_000);
    await expect(page.locator(".view .host canvas").first()).toBeVisible({ timeout: 60_000 });
    // Idle after opening, the page answers: nothing keeps recomputing the faces.
    await page.waitForTimeout(1000);
    const idle = await gapDuring(page, () => page.waitForTimeout(3000));
    expect(idle, "longest main-thread gap in ms after opening").toBeLessThan(2000);
    // Selecting a face recolours; it does not build the 3D scene again (UI-602).
    const view3d = page.locator(".view .host").first();
    const builds = await view3d.getAttribute("data-builds");
    const bb = (await page.locator("path.manual-face").first().boundingBox())!;
    await page.mouse.click(bb.x + bb.width / 2, bb.y + bb.height / 2);
    await expect(bar(page, "manual.face")).toBeVisible({ timeout: 30_000 });
    expect(await view3d.getAttribute("data-builds")).toBe(builds);
  });
});

test.describe("round-6 audit: the dormer dialog's checks run in turn (TEST-506 item 8)", () => {
  test("a width typed while the segment count is still checked is checked with the new count and refused", async ({ app, page }) => {
    const id = await createExample("gable", "Kulatý v pořadí");
    await openEditor(app, "Kulatý v pořadí");
    await tool(page, "tool.dormer").click();
    await clickPlan(page, 5, 2.4);
    const dlg = page.getByRole("dialog", { name: cs("dormer.dialogNew") });
    await expect(dlg).toBeVisible();
    await page.waitForTimeout(800);
    await dlg.getByRole("radio", { name: cs("dormer.round") }).click();
    await setField(dlg.getByLabel(cs("dormer.width")), "4");
    await page.waitForTimeout(500);
    // The segment count's check is slow; the width follows before it answers.
    let first = true;
    await page.route(/\/check_dormer$/, async (r: Route) => {
      if (first) {
        first = false;
        await new Promise((res) => setTimeout(res, 1500));
      }
      await r.continue();
    });
    await setField(dlg.getByLabel(cs("round.segments")), "10");
    await setField(dlg.getByLabel(cs("dormer.width")), "0,6");
    await expect(dlg.getByTestId("dormer-refusal")).toBeVisible({ timeout: 6000 });
    await expect(dlg.getByRole("button", { name: cs("common.ok"), exact: true })).toBeDisabled();
    expect((await loadProject(id)).dormers).toHaveLength(0);
  });
});
