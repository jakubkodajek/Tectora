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
});

test.describe("round-6 audit: keys during a drag (ERR-502)", () => {
  test("Delete and Ctrl+Z pressed while a dormer is dragged wait for the drop; after it they work on the dropped job", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "Klávesy při tahu");
    await threeDormers(mcp, id);
    await openAfterAi(app, "Klávesy při tahu");
    await selectDormer(page, 3);
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
    await holdUntil(page, "set_part_height", () => times.down !== undefined, 1200, times, 400);
    await typeInto(page, bar(page, "def.title").getByTestId("building-height"), "4");
    await page.keyboard.press("Enter");
    await page.waitForTimeout(200);
    const at = await planXY(page, c1.x, c1.y);
    await page.mouse.move(at.x, at.y);
    await page.mouse.down();
    times.down = Date.now();
    // Steps for about 4 s, each far enough to move the corner (past the 5 cm
    // snap and the alignment magnet): every step changes the job.
    for (let i = 1; i <= 40; i++) {
      await page.mouse.move(at.x + 20 + i * 4, at.y + 20 + i * 3);
      await page.waitForTimeout(70);
    }
    await page.mouse.up();
    times.up = Date.now();
    expect(times.answered, "the core answered while the corner was held").toBeGreaterThan(times.down);
    expect(times.answered!).toBeLessThan(times.up);
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
    await page.waitForTimeout(500);
    await expect(page.locator(".statusbar .save")).toHaveText(cs("editor.saving"));
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
    await ok.focus();
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
    test.setTimeout(180_000);
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
    // An edit is drawn again within a few seconds.
    await selectWall(page, 0);
    const t1 = Date.now();
    const pitch = bar(page, "def.title").getByLabel(cs("edge.pitch"));
    await typeInto(page, pitch, "38");
    await page.keyboard.press("Enter");
    await waitForProject(id, (p) => p.parts[0].edges[0].pitch_deg === 38, 30_000);
    const ping = Date.now();
    await page.evaluate(() => 1);
    expect(Date.now() - ping, "the page answers right after the edit").toBeLessThan(2000);
    expect(Date.now() - t1).toBeLessThan(30_000);
    // The 3D view opens it too.
    await page.keyboard.press("F3");
    await expect(page.locator(".view canvas").first()).toBeVisible({ timeout: 30_000 });
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
