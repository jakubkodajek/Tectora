// Guards for the round-5 audit (2026-11-22) findings in the editor, all of
// the one edit rule (app/src/lib/jobedits.ts): an edit belongs to its item,
// own edits never kill a hold, edits are serialized and wait for a drag,
// refusals are visible.
//
// ERR-401 an edit the core answers during an object or building drag;
// UI-403 a field reused for the next chimney, window, wall or distance;
// UI-401 Delete corner with an AI save while the question is open;
// UX-401 a refused edit stays visible; UX-402 an own delete is no "AI change";
// UX-403 a burst of ArrowUp shows the newest step; UI-402 focus on the
// status select; To check (55) the helper dialog stays open when refused;
// the library search with full-width letters.
import type { Locator, Page, Route } from "@playwright/test";
import { bar, clickPlan, openEditor, planXY, selectWall, tool } from "./editor";
import { bridge, createExample, expect, loadProject, test, waitForProject, type App } from "./fixtures";
import { cs } from "./i18n";

const square = (x: number, y: number, s: number) => [
  { x, y },
  { x: x + s, y },
  { x: x + s, y: y + s },
  { x, y: y + s },
];
const extent = (o: any, k: "x" | "y") => {
  const v = o.corners.map((c: any) => c[k]) as number[];
  return +(v.reduce((a, b) => Math.max(a, b)) - v.reduce((a, b) => Math.min(a, b))).toFixed(3);
};
const dormerWidth = (d: any) => +Math.hypot(d.end.x - d.edges[0].start.x, d.end.y - d.edges[0].start.y).toFixed(3);
const wallLength = (p: any, part: number, edge: number) => {
  const es = p.parts[part].edges;
  const a = es[edge].start;
  const b = es[(edge + 1) % es.length].start;
  return +Math.hypot(b.x - a.x, b.y - a.y).toFixed(3);
};
const placement = async (id: string, kind: string, index: number) => bridge<any>("item_placement", { project: await loadProject(id), kind, index });

async function withItems(id: string, edit: (p: any) => void) {
  const p = await loadProject(id);
  edit(p);
  await bridge("save_project", { id, project: p });
}

/** Types like a person: click, select all, type (no fill()). */
async function typeInto(page: Page, field: Locator, text: string) {
  await field.click();
  await field.press("ControlOrMeta+a");
  await page.keyboard.type(text);
}

/** Holds back the bridge's answers to `cmd` by `ms` while `on()` is true. */
async function slow(page: Page, cmd: string, ms: number, on: () => boolean = () => true) {
  await page.route(new RegExp(`/${cmd}$`), async (r: Route) => {
    if (on()) await new Promise((res) => setTimeout(res, ms));
    await r.continue();
  });
}

/**
 * Holds the bridge's answers to `cmd` until `gate()` is true (the pointer
 * is down), then `ms` more; `times.answered` is when the held answer went
 * back (TEST-505: the answer must really land during the drag).
 */
async function holdUntil(page: Page, cmd: string, gate: () => boolean, ms: number, times: { answered?: number }) {
  let first = true;
  await page.route(new RegExp(`/${cmd}$`), async (r: Route) => {
    if (first) {
      first = false;
      while (!gate()) await new Promise((res) => setTimeout(res, 20));
      await new Promise((res) => setTimeout(res, ms));
      const resp = await r.fetch();
      times.answered = Date.now();
      await r.fulfill({ response: resp });
    } else await r.continue();
  });
}

/** Every toast seen from now on, as `kind: text` (toasts close themselves). */
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
  await expect(app.page.locator(".view .wrap > svg").first()).toBeVisible();
}

async function selectDormer(page: Page, n: number) {
  const db = bar(page, "dormer.title");
  const bb = (await page.locator("g.dormer path.dormer-area").first().boundingBox())!;
  await page.mouse.click(bb.x + bb.width / 2, bb.y + bb.height / 2);
  await expect(db.locator(".which strong")).toBeVisible();
  for (let i = 0; i < 4 && (await db.locator(".which strong").textContent()) !== cs("dormer.n", { n: 1 }); i++) await db.getByRole("button", { name: cs("dormer.prev") }).click();
  for (let i = 1; i < n; i++) await db.getByRole("button", { name: cs("dormer.next") }).click();
  await expect(db.locator(".which strong")).toHaveText(cs("dormer.n", { n }));
  return db;
}

async function selectChimney(page: Page, n = 1) {
  const bb = (await page.locator("path.opening.chimney").nth(n - 1).boundingBox())!;
  await page.mouse.click(bb.x + bb.width / 2, bb.y + bb.height / 2);
  const ob = bar(page, "opening.chimney");
  await expect(ob.locator(".which strong")).toHaveText(cs("opening.chimneyN", { n }));
  return ob;
}

/**
 * Presses on `el`, moves a little, holds the pointer still for `stillMs`,
 * then drags on by `dx`, `dy` and lets go. `times.down` / `times.up` are
 * when the pointer went down and up.
 */
async function dragHoldingStill(page: Page, at: { x: number; y: number }, dx: number, dy: number, stillMs: number, during: () => void = () => {}, times: { down?: number; up?: number } = {}) {
  await page.mouse.move(at.x, at.y);
  await page.mouse.down();
  times.down = Date.now();
  await page.mouse.move(at.x + dx * 0.1, at.y + dy * 0.1, { steps: 2 });
  await page.mouse.move(at.x + dx * 0.2, at.y + dy * 0.2, { steps: 2 });
  await page.waitForTimeout(stillMs);
  during();
  for (let i = 3; i <= 10; i++) {
    await page.mouse.move(at.x + (dx * i) / 10, at.y + (dy * i) / 10);
    await page.waitForTimeout(40);
  }
  await page.mouse.up();
  times.up = Date.now();
}

// With TECTORA_SHOTS=<folder>/ and TECTORA_SHOTS_TAG=before|after every test
// leaves a screenshot `r5-<finding>-<n>-<tag>.png` there; without it nothing is written.
let shotNo = 0;
test.afterEach(async ({ page }, info) => {
  const dir = process.env.TECTORA_SHOTS;
  if (!dir) return;
  const id = info.title.match(/\(([A-Z]+-\d+)/)?.[1] ?? "misc";
  await page.screenshot({ path: `${dir}r5-${id}-${++shotNo}-${process.env.TECTORA_SHOTS_TAG ?? "after"}.png` }).catch(() => {});
});

test.describe("round-5 audit: an edit answered during a drag", () => {
  test("a dormer width answered while another dormer is dragged is kept after the drag (ERR-401)", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "Tah vikýře");
    await threeDormers(mcp, id);
    await openAfterAi(app, "Tah vikýře");
    const db = await selectDormer(page, 2);
    const toasts = await watchToasts(page);
    // The width's check is answered only once the pointer is down and held
    // still: the answer lands during the drag, every run (TEST-505).
    const times: { answered?: number; down?: number; up?: number } = {};
    await holdUntil(page, "check_dormer", () => times.down !== undefined, 1000, times);
    await typeInto(page, db.getByLabel(cs("dormer.width")), "2");
    await page.keyboard.press("Enter");
    await page.waitForTimeout(200);
    // Dormer 3 (front 8.5 to 10 m at y = 1.6, into the roof): the answer
    // comes while the pointer is held still, and more drag steps follow.
    await dragHoldingStill(page, await planXY(page, 9.25, 2.1), 0, 60, 4000, () => {}, times);
    expect(times.answered, "the core answered while the pointer was down").toBeGreaterThan(times.down!);
    expect(times.answered!).toBeLessThan(times.up!);
    // The end of the drag (60 px down is past y = 1 m), not a file saved
    // half way: no autosave runs during a drag any more.
    const p = await waitForProject(id, (p) => dormerWidth(p.dormers[1]) === 2 && p.dormers[2].edges[0].start.y < 1, 12_000);
    await page.waitForTimeout(1500);
    await app.saved();
    const q = await loadProject(id);
    expect(q.dormers.map(dormerWidth)).toEqual([1.5, 2, 1.5]);
    expect(q.dormers[2].edges[0].start.y).toBe(p.dormers[2].edges[0].start.y);
    expect((await toasts()).filter((t) => !t.startsWith("info"))).toEqual([]);
  });

  test("a building height answered while the building is dragged is kept after the drag (ERR-401)", async ({ app, page }) => {
    const id = await createExample("gable", "Tah budovy");
    await openEditor(app, "Tah budovy");
    const before = await loadProject(id);
    const x0 = before.parts[0].edges[0].start.x;
    const def = bar(page, "def.title");
    await selectWall(page, 0);
    const toasts = await watchToasts(page);
    // Answered only once the pointer is down: without the drag pause the
    // drop would write the old height back (TEST-505, mutant E5).
    const times: { answered?: number; down?: number; up?: number } = {};
    await holdUntil(page, "set_part_height", () => times.down !== undefined, 1000, times);
    await typeInto(page, def.getByTestId("building-height"), "4");
    await page.keyboard.press("Enter");
    await page.waitForTimeout(200);
    // On the front face, off the ridge line.
    await dragHoldingStill(page, await planXY(page, 3, 2), 80, 0, 4000, () => {}, times);
    expect(times.answered, "the core answered while the pointer was down").toBeGreaterThan(times.down!);
    expect(times.answered!).toBeLessThan(times.up!);
    await waitForProject(id, (p) => p.parts[0].edges[0].start.x !== x0 && p.parts[0].edges[0].eave_height !== before.parts[0].edges[0].eave_height, 12_000);
    await page.waitForTimeout(1500);
    await app.saved();
    const q = await loadProject(id);
    expect(q.parts[0].height ?? q.parts[0].edges[0].eave_height).toBe(4);
    expect(q.parts[0].edges[0].start.x).not.toBe(x0);
    expect((await toasts()).filter((t) => !t.startsWith("info"))).toEqual([]);
  });
});

test.describe("round-5 audit: a field reused for the next item", () => {
  test("the same chimney width typed on the next chimney while the first is with the core lands on both (UI-403)", async ({ app, page }) => {
    const id = await createExample("gable", "Dva komíny");
    await withItems(id, (p) => (p.chimneys = [{ corners: square(3, 1.5, 1.4), height: 8 }, { corners: square(8, 1.5, 0.6), height: 8 }]));
    await openEditor(app, "Dva komíny");
    const ob = await selectChimney(page, 1);
    await slow(page, "item_geometry", 1500);
    await typeInto(page, ob.getByLabel(cs("opening.width")), "0,9");
    await page.keyboard.press("Enter");
    await page.keyboard.press("PageDown");
    await expect(ob.locator(".which strong")).toHaveText(cs("opening.chimneyN", { n: 2 }));
    await page.keyboard.press("ControlOrMeta+a");
    await page.keyboard.type("0,9");
    await page.keyboard.press("Enter");
    await waitForProject(id, (p) => extent(p.chimneys[0], "x") === 0.9 && extent(p.chimneys[1], "x") === 0.9, 12_000);
  });

  test("one ArrowUp on the next roof window of the same size is one step, not two (UI-403)", async ({ app, page }) => {
    const id = await createExample("gable", "Dvě okna");
    await withItems(id, (p) => (p.windows = [{ x: 3, y: 1.5, width: 0.78, height: 1.18 }, { x: 7, y: 1.5, width: 0.78, height: 1.18 }]));
    await openEditor(app, "Dvě okna");
    await clickPlan(page, 3.4, 1.9);
    const ob = bar(page, "opening.window");
    await expect(ob.locator(".which strong")).toHaveText(cs("opening.windowN", { n: 1 }));
    const width = ob.getByLabel(cs("opening.width"));
    await width.click();
    await width.press("ArrowUp");
    await page.keyboard.press("PageDown");
    await expect(ob.locator(".which strong")).toHaveText(cs("opening.windowN", { n: 2 }));
    await page.keyboard.press("ArrowUp");
    await waitForProject(id, (p) => p.windows[0].width === 0.83 && p.windows[1].width !== 0.78, 10_000);
    await page.waitForTimeout(2000);
    await app.saved();
    expect((await loadProject(id)).windows.map((w: any) => w.width)).toEqual([0.83, 0.83]);
    await expect(width).toHaveValue("0,83");
  });

  test("the same wall length typed on the next wall while the first is with the core lands on both (UI-403)", async ({ app, page }) => {
    const id = await createExample("gable", "Dvě délky");
    await openEditor(app, "Dvě délky");
    const def = bar(page, "def.title");
    await selectWall(page, 0);
    await slow(page, "item_geometry", 1500);
    await typeInto(page, def.getByLabel(cs("edge.length")), "9");
    await page.keyboard.press("Enter");
    await page.keyboard.press("PageDown");
    await expect(def.locator(".which strong")).toHaveText(cs("def.wall", { n: 2 }));
    await page.keyboard.press("ControlOrMeta+a");
    await page.keyboard.type("9");
    await page.keyboard.press("Enter");
    await waitForProject(id, (p) => wallLength(p, 0, 0) === 9 && wallLength(p, 0, 1) === 9, 12_000);
  });

  test("the same distance from the eave typed on the next chimney lands on both (UI-403)", async ({ app, page }) => {
    const id = await createExample("gable", "Dvě vzdálenosti");
    await withItems(id, (p) => (p.chimneys = [{ corners: square(3, 1.5, 0.6), height: 8 }, { corners: square(8, 1, 0.6), height: 8 }]));
    await openEditor(app, "Dvě vzdálenosti");
    const ob = await selectChimney(page, 1);
    await slow(page, "place_item", 1500);
    await typeInto(page, ob.getByTestId("place-eave"), "1,5");
    await page.keyboard.press("Enter");
    await page.keyboard.press("PageDown");
    await expect(ob.locator(".which strong")).toHaveText(cs("opening.chimneyN", { n: 2 }));
    await page.keyboard.press("ControlOrMeta+a");
    await page.keyboard.type("1,5");
    await page.keyboard.press("Enter");
    await expect
      .poll(async () => [+(await placement(id, "chimney", 0)).from_eave.toFixed(3), +(await placement(id, "chimney", 1)).from_eave.toFixed(3)], { timeout: 12_000 })
      .toEqual([1.5, 1.5]);
  });
});

test.describe("round-5 audit: refusals are visible, own deletes are not refusals", () => {
  test("Delete corner answered after an AI save of the job data deletes the corner (UI-401)", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "Roh a AI");
    await openEditor(app, "Roh a AI");
    const corners = (p: any) => p.parts[0].edges.map((e: any) => [e.start.x, e.start.y]);
    const before = corners(await loadProject(id));
    await selectWall(page, 1);
    const toasts = await watchToasts(page);
    // Tauri's confirm is async, and the user takes a moment to answer.
    await page.evaluate(() => {
      (window as any).confirm = () => new Promise((r) => setTimeout(() => r(true), 2500));
    });
    await page.locator(".view .wrap > svg").first().focus().catch(() => {});
    await page.keyboard.press("Delete");
    await page.waitForTimeout(400);
    await mcp.call("update_job", { id, number: "R5-del", summary: "AI sets the order number." });
    const p = await waitForProject(id, (p) => p.parts[0].edges.length === 3, 12_000);
    expect(p.job.number).toBe("R5-del");
    // The corner asked about (the start of wall 2) is the one gone, not another (TEST-505, E3).
    expect(corners(p)).toEqual(before.filter((_: unknown, i: number) => i !== 1));
    expect((await toasts()).filter((t) => t.includes(cs("editor.actionStale")))).toEqual([]);
  });

  test("deleting a chimney while its own width is with the core says nothing about an AI change (UX-402)", async ({ app, page }) => {
    const id = await createExample("gable", "Komín smazán");
    await withItems(id, (p) => (p.chimneys = [{ corners: square(5, 1.5, 0.6), height: 8 }]));
    await openEditor(app, "Komín smazán");
    const ob = await selectChimney(page, 1);
    const toasts = await watchToasts(page);
    await slow(page, "item_geometry", 1500);
    await typeInto(page, ob.getByLabel(cs("opening.width")), "0,9");
    await page.keyboard.press("Enter");
    await ob.getByRole("button", { name: cs("opening.deleteChimney") }).click();
    await waitForProject(id, (p) => p.chimneys.length === 0);
    await page.waitForTimeout(2500);
    expect((await loadProject(id)).chimneys).toEqual([]);
    expect((await toasts()).filter((t) => t.includes(cs("editor.actionStale")) || t.includes(cs("editor.actionBusy")))).toEqual([]);
  });

  test("an edit refused because the AI changed its chimney is a warning naming the chimney, and the field stays marked (UX-401)", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "Komín změněn");
    await withItems(id, (p) => (p.chimneys = [{ corners: square(3, 1.5, 0.6), height: 8 }, { corners: square(8, 1.5, 0.6), height: 8 }]));
    await openEditor(app, "Komín změněn");
    const ob = await selectChimney(page, 2);
    let delay = true;
    await slow(page, "item_geometry", 2500, () => delay);
    const width = ob.getByLabel(cs("opening.width"));
    await typeInto(page, width, "0,9");
    await page.keyboard.press("Enter");
    await page.waitForTimeout(400);
    // The AI changes the very chimney meanwhile.
    await mcp.call("update_job", { id, notes: "AI", summary: "AI note." });
    await withItems(id, (p) => (p.chimneys[1].height = 9));
    delay = false;
    const toast = app.toasts().filter({ hasText: cs("editor.actionStale") });
    await expect(toast).toBeVisible({ timeout: 12_000 });
    await expect(toast).toHaveClass(/warning/);
    await expect(toast).toContainText(cs("editor.notMadeOf", { name: cs("opening.chimneyN", { n: 2 }) }));
    await expect(width).toHaveAttribute("aria-invalid", "true");
    await page.waitForTimeout(5000);
    // Still marked, with the reason, until the user types in it again.
    await expect(width).toHaveAttribute("aria-invalid", "true");
    await expect(width).toHaveAttribute("title", cs("editor.actionStale"));
    await width.press("End");
    await page.keyboard.type("5");
    await expect(width).not.toHaveAttribute("aria-invalid", "true");
    expect(extent((await loadProject(id)).chimneys[1], "x")).toBe(0.6);
  });
});

test.describe("round-5 audit: fields and focus", () => {
  test("holding ArrowUp on the building height shows the newest step, never an earlier one (UX-403)", async ({ app, page }) => {
    const id = await createExample("gable", "Kroky výšky");
    await openEditor(app, "Kroky výšky");
    const def = bar(page, "def.title");
    await selectWall(page, 0);
    const h0 = (await loadProject(id)).parts[0].edges[0].eave_height as number;
    await slow(page, "set_part_height", 300);
    const height = def.getByTestId("building-height");
    await height.click();
    const seen: number[] = [];
    // The field's text sampled every 20 ms from the page itself while 8 presses go out.
    await page.evaluate(() => {
      const w = window as any;
      const f = document.querySelector<HTMLInputElement>('[data-testid="building-height"]')!;
      w.__seen = [];
      w.__sample = setInterval(() => w.__seen.push(f.value), 20);
    });
    for (let i = 0; i < 8; i++) await height.press("ArrowUp");
    await page.waitForTimeout(3000);
    seen.push(...(await page.evaluate(() => (clearInterval((window as any).__sample), (window as any).__seen as string[]))).map((v) => +v.replace(",", ".")));
    const end = +(h0 + 0.4).toFixed(3);
    await waitForProject(id, (p) => p.parts[0].edges[0].eave_height === end, 12_000);
    for (let i = 1; i < seen.length; i++) expect(seen[i], seen.join(" ")).toBeGreaterThanOrEqual(seen[i - 1]);
    await expect(height).toHaveValue(String(end).replace(".", ","));
  });

  test("the status select keeps the keyboard focus while the result is stored (UI-402)", async ({ app, page }) => {
    const id = await createExample("gable", "Stav klávesnicí");
    await openEditor(app, "Stav klávesnicí");
    await slow(page, "take_snapshot", 800);
    await page.locator("aside .tabs").getByRole("tab", { name: cs("dock.job") }).click();
    const select = page.locator("aside .job select");
    await select.focus();
    // Every status the select chose, also one it took back at once.
    await select.evaluate((el) => {
      (window as any).__chosen = [];
      el.addEventListener("input", () => (window as any).__chosen.push((el as HTMLSelectElement).value), true);
    });
    await page.keyboard.press("ArrowDown");
    await page.waitForTimeout(300);
    expect(await page.evaluate(() => document.activeElement?.tagName)).toBe("SELECT");
    // While the first status is stored, a second key chooses nothing (TEST-505, E8).
    await expect(select).toHaveAttribute("aria-disabled", "true");
    await page.keyboard.press("ArrowDown");
    // The second key chose nothing, not even for a moment.
    expect(await select.inputValue()).toBe("offer");
    expect(await page.evaluate(() => (window as any).__chosen)).toEqual(["offer"]);
    await waitForProject(id, (p) => p.job.status === "offer" && !!p.result_snapshot, 10_000);
    await page.waitForTimeout(1200);
    expect(await page.evaluate(() => document.activeElement?.tagName)).toBe("SELECT");
    await expect(select).not.toHaveAttribute("aria-disabled", "true");
    await expect(select).toHaveValue("offer");
    expect((await loadProject(id)).job.status).toBe("offer");
  });

  test("the calculation helper stays open with its values when the core refuses it (ERR-055)", async ({ app, page }) => {
    const id = await createExample("gable", "Pomocník odmítnut");
    await openEditor(app, "Pomocník odmítnut");
    const e0 = (await loadProject(id)).parts[0].edges[0];
    await selectWall(page, 0);
    await page.route(/\/roof_helper_apply$/, (r: Route) => r.fulfill({ contentType: "application/json", body: JSON.stringify({ err: { code: "refused", detail: "the helper gives values a wall does not take" } }) }));
    await bar(page, "def.title").getByRole("button", { name: cs("calc.button") }).click();
    const dlg = page.getByRole("dialog", { name: cs("calc.title") });
    await dlg.getByRole("tab", { name: cs("calc.buildup") }).click();
    const thick = dlg.getByRole("textbox", { name: cs("calc.thickness") });
    await thick.fill("0,3");
    await thick.press("Tab");
    await dlg.getByRole("button", { name: cs("common.ok") }).click();
    await expect(app.toasts().filter({ hasText: cs("refuse.helperRange") })).toBeVisible();
    await expect(dlg).toBeVisible();
    await expect(thick).toHaveValue("0,3");
    expect((await loadProject(id)).parts[0].edges[0].overhang).toBe(e0.overhang);
  });
});

test.describe("round-5 audit: the library", () => {
  test("the search finds a customer typed in full-width letters, as the AI's search does", async ({ app, page }) => {
    const id = await createExample("gable", "Široká písmena");
    await withItems(id, (p) => (p.job.customer = "Novák Střechy"));
    await app.open();
    await page.getByRole("searchbox").fill("ＮＯＶＡＫ ｓｔｒｅｃｈｙ");
    await expect(page.getByRole("row").filter({ hasText: "Široká písmena" })).toBeVisible();
    void tool;
  });
});
