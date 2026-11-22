// Guards for the round-7 audit (2026-11-22): a drag in the plan is one
// transaction (app/src/lib/dragsession.ts, jobedits.ts rule 7). The matrix:
//
//   drag kind  object (a dormer), building, corner
//   phase      pressed and not moved, moving, the drop held with the core
//   intruder   Delete, Ctrl+Z, an arrow, Tab + Enter, a focused button, a
//              typed value; a second finger on an item / a wall / a corner /
//              a building / the background and a mouse click on a button
//              during a touch drag; an AI save of this job; show_project of
//              another job (follow mode); Escape; leaving the job
//
// After every cell: (a) the file holds exactly the drop (Escape: the job of
// the press); (b) the user's intruder did nothing, what is not the user's
// landed after the drop; (c) a value typed afterwards lands within 3 s, the
// status says saved, Back leaves; (d) one Undo gives the job before the
// drag, Redo the drop.
//
// UI-601 (a second press replaces the drag), UI-603 (keys before the slop),
// UI-605 / ERR-604 (buttons and fields during a drag), ERR-601 (follow mode
// during a corner drag), ERR-602 (keys are refused, not replayed: owner
// decision), ERR-603 (Undo right after a building's drop), UX-604 (the hint
// names Escape). The reviewers' scripts these cells come from: errors
// follow.cjs, keydrag.cjs, undopart.cjs, focusbtn.cjs; ui t601-touch.mjs,
// t602-keys.mjs, t612-touchbtn.mjs, t615-typedrag.mjs.
import type { Page, Route } from "@playwright/test";
import { bar, plan, planXY, tool } from "./editor";
import { bridge, createExample, expect, loadProject, test, waitForProject, type App } from "./fixtures";
import { cs } from "./i18n";

type Kind = "object" | "building" | "corner";
type XY = { x: number; y: number };

const dormerWidth = (d: any) => +Math.hypot(d.end.x - d.edges[0].start.x, d.end.y - d.edges[0].start.y).toFixed(3);
/** What a drag may change, as text: the buildings and every object on them. */
const shape = (p: any) => JSON.stringify({ parts: p.parts, dormers: p.dormers, chimneys: p.chimneys ?? [], windows: p.windows ?? [] });

async function acceptAll(id: string) {
  const log = await bridge<{ ts_ms: number }[]>("project_activity", { id });
  if (log.length) await bridge("accept_changes", { id, tsMs: log.reduce((m, a) => Math.max(m, a.ts_ms), 0) });
}

/** The centre of dormer `i` (0-based) in the plan, in page pixels. */
async function dormerAt(page: Page, i: number): Promise<XY> {
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

async function typeInto(page: Page, field: ReturnType<Page["locator"]>, text: string) {
  await field.click();
  await field.press("ControlOrMeta+a");
  await page.keyboard.type(text);
}

/** Holds the bridge's answers to `cmd` by `ms` while `on()` is true. */
async function slow(page: Page, cmd: string, ms: number, on: () => boolean) {
  await page.route(new RegExp(`/${cmd}$`), async (r: Route) => {
    if (on()) await new Promise((res) => setTimeout(res, ms));
    await r.continue();
  });
}

/**
 * A gable roof 12 x 8 with three dormers, open in the editor, with one step
 * in the history (dormer 1 made 2 m wide): a Ctrl+Z that got through during
 * a drag would undo it. Returns the job as the file has it before the drag.
 */
async function stage(app: App, page: Page, mcp: any, name: string) {
  const id = await createExample("gable", name);
  for (const x of [1.5, 5, 8.5]) await mcp.call("add_dormer", { id, kind: "gable", start: [x, 1.6], width: 1.5, summary: "Dormer." });
  await acceptAll(id);
  await app.open();
  const editor = page.locator(".editor .statusbar");
  if (!(await editor.isVisible().catch(() => false))) await page.getByRole("row").filter({ hasText: name }).first().dblclick({ timeout: 3000 }).catch(() => {});
  await expect(editor).toBeVisible();
  await expect(plan(page)).toBeVisible();
  const db = await selectDormer(page, 1);
  await typeInto(page, db.getByLabel(cs("dormer.width")), "2");
  await page.keyboard.press("Enter");
  await waitForProject(id, (p) => dormerWidth(p.dormers[0]) === 2);
  await app.saved();
  return { id, pre: await loadProject(id) };
}

/** Where the press of a drag of `kind` goes, and the step it moves by; selects what is dragged. */
async function aim(page: Page, kind: Kind): Promise<{ at: XY; step: XY }> {
  if (kind === "object") {
    await selectDormer(page, 3);
    // Past the double-press time: a second press within it opens the dormer window.
    await page.waitForTimeout(600);
    return { at: await dormerAt(page, 2), step: { x: 0, y: 4 } };
  }
  if (kind === "building") {
    // The slope without dormers; the first press only selects the building.
    const at = await planXY(page, 3, 6.5);
    await page.mouse.click(at.x, at.y);
    await expect(page.locator("path.fill.movable")).toHaveCount(1);
    await page.waitForTimeout(600);
    return { at, step: { x: 5, y: 0 } };
  }
  const at = await planXY(page, 12, 0);
  return { at, step: { x: 5, y: 4 } };
}

/** Presses at `at` and holds. */
async function pressAt(page: Page, at: XY) {
  await page.mouse.move(at.x, at.y);
  await page.mouse.down();
}

/** Moves the held pointer through steps `from`..`to` of `step` px each. */
async function moveSteps(page: Page, a: { at: XY; step: XY }, from: number, to: number) {
  for (let i = from; i <= to; i++) {
    await page.mouse.move(a.at.x + a.step.x * i, a.at.y + a.step.y * i);
    await page.waitForTimeout(30);
  }
}

/** The dragged thing differs from the job before the drag. */
const moved = (kind: Kind, pre: any) => (p: any) =>
  kind === "object" ? JSON.stringify(p.dormers[2]) !== JSON.stringify(pre.dormers[2]) : JSON.stringify(p.parts[0].edges) !== JSON.stringify(pre.parts[0].edges);

/** (a) Only what was dragged differs from the job before the drag. */
function onlyTheDrop(kind: Kind, pre: any, now: any) {
  expect(now.dormers.length).toBe(3);
  expect(now.dormers.map(dormerWidth)).toEqual([2, 1.5, 1.5]);
  if (kind === "object") {
    expect(JSON.stringify(now.parts)).toBe(JSON.stringify(pre.parts));
    expect(JSON.stringify(now.dormers.slice(0, 2))).toBe(JSON.stringify(pre.dormers.slice(0, 2)));
  } else if (kind === "building") {
    // Every corner moved by the same step: the building as a whole.
    const d = now.parts[0].edges.map((e: any, i: number) => [+(e.start.x - pre.parts[0].edges[i].start.x).toFixed(3), +(e.start.y - pre.parts[0].edges[i].start.y).toFixed(3)]);
    for (const v of d) expect(v).toEqual(d[0]);
    expect(d[0][0]).toBeGreaterThan(0);
  } else {
    // Only corner 2 moved.
    const es = now.parts[0].edges;
    expect(es.length).toBe(pre.parts[0].edges.length);
    es.forEach((e: any, i: number) => {
      if (i !== 1) expect(e.start).toEqual(pre.parts[0].edges[i].start);
    });
    expect(JSON.stringify(now.dormers)).toBe(JSON.stringify(pre.dormers));
  }
}

/**
 * The user's intruders, one after another, while the drag is open. Each
 * would change the job if it got through: Delete has its confirm answered
 * Yes beforehand, Ctrl+Z has a step to undo, the arrow nudges the selected
 * object, the buttons delete, the typed value is a pitch or a width.
 */
async function intrude(app: App, page: Page, kind: Kind) {
  const nothing = async (what: string) => {
    expect(await app.dialogs(), `${what}: no question asked`).toEqual([]);
    await expect(page.locator("g.dormer"), `${what}: no dormer removed`).toHaveCount(3);
    // The focus is nowhere in the closed bars, toolbar, dock or header.
    expect(await page.evaluate(() => !!document.activeElement?.closest(".toolbar, .bar, aside, header, footer")), `${what}: focus outside the closed controls`).toBe(false);
  };
  await app.answerConfirm(true, true, true, true);
  await page.keyboard.press("Delete");
  await nothing("Delete");
  await page.keyboard.press("ControlOrMeta+z");
  await nothing("Ctrl+Z");
  await page.keyboard.press("ArrowRight");
  await page.keyboard.press("ControlOrMeta+d");
  await nothing("arrow, Ctrl+D");
  // Tab to a button and Enter (ui t612 `kbd`).
  for (let i = 0; i < 3; i++) await page.keyboard.press("Tab");
  await page.keyboard.press("Enter");
  await nothing("Tab + Enter");
  // A button focused by script and pressed from the keyboard (errors focusbtn.cjs).
  const del = kind === "object" ? bar(page, "dormer.title").getByRole("button", { name: cs("dormer.delete") }) : tool(page, "tool.deletePart");
  await del.evaluate((el: HTMLElement) => el.focus());
  await page.keyboard.press("Space");
  await page.keyboard.press("Enter");
  await nothing("focused Delete button + Space / Enter");
  // A value typed into the bar's first field (ui t615).
  await page.evaluate(() => document.querySelector<HTMLInputElement>(".editor .main .bar input:not(:disabled)")?.focus());
  await page.keyboard.type("33");
  await page.keyboard.press("Enter");
  await nothing("typed value");
  // The controls are closed for the mouse and the keyboard, not only by our guards.
  for (const sel of ["header", ".toolbar", "aside", ".main .bar"]) await expect(page.locator(`.editor ${sel}`).first()).toHaveAttribute("inert", "");
}

/** (d) One Undo gives the job before the drag, Redo the drop. */
async function undoRedo(app: App, page: Page, id: string, pre: any, dropped: any) {
  await page.keyboard.press("ControlOrMeta+z");
  await waitForProject(id, (p) => shape(p) === shape(pre), 6000);
  await expect(tool(page, "editor.redo")).toBeEnabled();
  await tool(page, "editor.redo").click();
  await waitForProject(id, (p) => shape(p) === shape(dropped), 6000);
  await expect(tool(page, "editor.redo")).toBeDisabled();
}

/** (c) A value typed afterwards lands within 3 s, the status says saved, Back leaves. */
async function probe(app: App, page: Page, id: string) {
  for (const sel of ["header", ".toolbar", "aside"]) await expect(page.locator(`.editor ${sel}`).first()).not.toHaveAttribute("inert", "");
  const db = await selectDormer(page, 2);
  await typeInto(page, db.getByLabel(cs("dormer.width")), "1,8");
  await page.keyboard.press("Enter");
  await waitForProject(id, (p) => dormerWidth(p.dormers[1]) === 1.8, 3000);
  await app.saved();
  await page.locator("header .icon-btn").first().click();
  await expect(page.locator(".editor")).toBeHidden({ timeout: 5000 });
}

const KINDS: Kind[] = ["object", "building", "corner"];
const NAME: Record<Kind, string> = { object: "a dormer", building: "the building", corner: "a corner" };

test.afterEach(async ({ page }, info) => {
  const dir = process.env.TECTORA_SHOTS;
  if (!dir) return;
  const n = info.title.replace(/[^a-z0-9]+/gi, "-").slice(0, 40);
  await page.screenshot({ path: `${dir}r7-drag-${n}-${process.env.TECTORA_SHOTS_TAG ?? "after"}.png`, timeout: 10_000 }).catch(() => {});
});

test.describe("round-7 audit: user input during a drag is refused (UI-603, UI-605, ERR-602, ERR-604)", () => {
  for (const kind of KINDS) {
    test(`${NAME[kind]} pressed and not yet moved: keys, buttons and typed values do nothing, and the press hurt nothing`, async ({ app, page, mcp }) => {
      const { id, pre } = await stage(app, page, mcp, `Stisk ${kind}`);
      const a = await aim(page, kind);
      await pressAt(page, a.at);
      await intrude(app, page, kind);
      await page.mouse.up();
      await page.waitForTimeout(1200);
      await app.saved();
      // (a) Nothing moved, nothing deleted, nothing undone.
      expect(shape(await loadProject(id))).toBe(shape(pre));
      // The editor is open again: the step before the press is the one Undo takes.
      await page.keyboard.press("ControlOrMeta+z");
      await waitForProject(id, (p) => dormerWidth(p.dormers[0]) === 1.5, 6000);
      await tool(page, "editor.redo").click();
      await waitForProject(id, (p) => shape(p) === shape(pre), 6000);
      await probe(app, page, id);
    });

    test(`${NAME[kind]} being dragged: keys, buttons and typed values do nothing; the file holds exactly the drop, one Undo takes it back`, async ({ app, page, mcp }) => {
      const { id, pre } = await stage(app, page, mcp, `Tah ${kind}`);
      const a = await aim(page, kind);
      await pressAt(page, a.at);
      await moveSteps(page, a, 1, 6);
      // The hint says how to get out (UX-604).
      await expect(page.getByTestId("plan-hint")).toHaveText(cs("plan.hintDrag"));
      await intrude(app, page, kind);
      await moveSteps(page, a, 7, 9);
      await page.mouse.up();
      const dropped = await waitForProject(id, moved(kind, pre), 6000);
      await app.saved();
      await expect(page.getByTestId("plan-hint")).toHaveText(cs("plan.hint"));
      onlyTheDrop(kind, pre, dropped);
      // Nothing of the refused input happens after the drop either (ERR-602: not replayed).
      await page.waitForTimeout(800);
      expect(await app.dialogs()).toEqual([]);
      expect(shape(await loadProject(id))).toBe(shape(dropped));
      await undoRedo(app, page, id, pre, dropped);
      await probe(app, page, id);
    });
  }

  test("Delete on the pressed dormer does not delete it, and the move drags that dormer, never its neighbour (UI-603)", async ({ app, page, mcp }) => {
    const { id, pre } = await stage(app, page, mcp, "Stisk a Delete");
    await selectDormer(page, 2);
    await page.waitForTimeout(600);
    const at = await dormerAt(page, 1);
    await pressAt(page, at);
    await app.answerConfirm(true);
    await page.keyboard.press("Delete");
    await page.waitForTimeout(300);
    for (let i = 1; i <= 8; i++) {
      await page.mouse.move(at.x, at.y + i * 5);
      await page.waitForTimeout(30);
    }
    await page.mouse.up();
    const p = await waitForProject(id, (q) => JSON.stringify(q.dormers[1]) !== JSON.stringify(pre.dormers[1]), 6000);
    expect(await app.dialogs()).toEqual([]);
    expect(p.dormers.length).toBe(3);
    // Dormer 3, never touched, is where it was.
    expect(p.dormers[2]).toEqual(pre.dormers[2]);
    expect(p.dormers[0]).toEqual(pre.dormers[0]);
  });
});

test.describe("round-7 audit: the drop with the core keeps the editor closed; Undo runs right after it (ERR-603)", () => {
  for (const kind of ["building", "corner"] as const) {
    test(`${NAME[kind]} dropped, the core's answer held: the status says saving, keys do nothing, Ctrl+Z undoes exactly the drop and Redo brings it back`, async ({ app, page, mcp }) => {
      const { id, pre } = await stage(app, page, mcp, `Usazení ${kind}`);
      const a = await aim(page, kind);
      let hold = false;
      // A building's move is `transform_part`, a corner's merge `item_geometry`.
      await slow(page, kind === "building" ? "transform_part" : "item_geometry", 1800, () => hold);
      await pressAt(page, a.at);
      await moveSteps(page, a, 1, 9);
      hold = true;
      await page.mouse.up();
      await page.waitForTimeout(250);
      // Closed and saying so, without waiting for it (no retry).
      expect(await page.locator(".statusbar .save").textContent()).toBe(cs("editor.saving"));
      await expect(page.locator(".editor .toolbar")).toHaveAttribute("inert", "");
      await app.answerConfirm(true, true);
      await page.keyboard.press("Delete");
      await page.keyboard.press("ArrowRight");
      await page.keyboard.type("33");
      // A new press in the plan does nothing either.
      const d1 = await dormerAt(page, 0);
      await page.mouse.click(d1.x, d1.y);
      expect(await app.dialogs()).toEqual([]);
      // The one thing that is queued: Undo, pressed while the move is still out.
      await page.keyboard.press("ControlOrMeta+z");
      hold = false;
      await page.waitForTimeout(3200);
      await app.saved();
      // One Undo took back exactly the drop: the step before it (dormer 1's width) is still there.
      expect(shape(await loadProject(id))).toBe(shape(pre));
      expect(await app.dialogs()).toEqual([]);
      await expect(tool(page, "editor.redo")).toBeEnabled();
      await tool(page, "editor.redo").click();
      const dropped = await waitForProject(id, moved(kind, pre), 6000);
      onlyTheDrop(kind, pre, dropped);
      await expect(tool(page, "editor.redo")).toBeDisabled();
      await probe(app, page, id);
    });
  }
});

test.describe("round-7 audit: Escape cancels a drag of every kind (UX-604, TEST-601)", () => {
  for (const kind of KINDS) {
    test(`Escape while ${NAME[kind]} is dragged puts the job back, saves nothing, and the rest of the move does nothing`, async ({ app, page, mcp }) => {
      const { id, pre } = await stage(app, page, mcp, `Esc ${kind}`);
      const a = await aim(page, kind);
      const before = await page.locator(kind === "object" ? "g.dormer >> nth=2" : "path.fill").first().boundingBox();
      await pressAt(page, a.at);
      await moveSteps(page, a, 1, 7);
      await expect(page.getByTestId("plan-hint")).toHaveText(cs("plan.hintDrag"));
      await page.keyboard.press("Escape");
      await expect(page.getByTestId("plan-hint")).toHaveText(cs("plan.hint"));
      await moveSteps(page, a, 8, 12);
      await page.mouse.up();
      await page.waitForTimeout(1500);
      await app.saved();
      expect(shape(await loadProject(id))).toBe(shape(pre));
      const after = await page.locator(kind === "object" ? "g.dormer >> nth=2" : "path.fill").first().boundingBox();
      expect(Math.abs(after!.x - before!.x) + Math.abs(after!.y - before!.y) + Math.abs(after!.width - before!.width)).toBeLessThan(3);
      // No step was added: Undo takes the step before the press.
      await page.keyboard.press("ControlOrMeta+z");
      await waitForProject(id, (p) => dormerWidth(p.dormers[0]) === 1.5, 6000);
      await probe(app, page, id);
    });
  }
});

test.describe("round-7 audit: what is not the user's waits for the end of the drag (ERR-601)", () => {
  for (const kind of KINDS) {
    test(`an AI save of this job while ${NAME[kind]} is dragged is read after the drop: both are in the file`, async ({ app, page, mcp }) => {
      const { id, pre } = await stage(app, page, mcp, `AI při tahu ${kind}`);
      const a = await aim(page, kind);
      await pressAt(page, a.at);
      await moveSteps(page, a, 1, 6);
      await mcp.call("update_job", { id, notes: "AI during the drag", summary: "AI note." });
      await page.waitForTimeout(900);
      // Not read while the drag is open: no review banner yet, the drag goes on.
      await expect(page.locator(".review")).toHaveCount(0);
      await moveSteps(page, a, 7, 9);
      await page.mouse.up();
      const p = await waitForProject(id, (q) => moved(kind, pre)(q) && q.notes === "AI during the drag", 8000);
      onlyTheDrop(kind, pre, p);
      await expect(page.locator(".review")).toBeVisible();
      await app.saved();
      expect((await loadProject(id)).notes).toBe("AI during the drag");
      await probe(app, page, id);
    });

    test(`show_project of another job while ${NAME[kind]} is dragged only offers it: the editor stays and the drop lands where it was let go`, async ({ app, page, mcp }) => {
      const { id, pre } = await stage(app, page, mcp, `Sledování ${kind}`);
      const other = await createExample("hip", "Jiná zakázka");
      const a = await aim(page, kind);
      await pressAt(page, a.at);
      await moveSteps(page, a, 1, 5);
      await mcp.call("show_project", { id: other });
      const offer = app.toasts().filter({ hasText: cs("ai.followOffer", { who: "Claude" }) });
      await expect(offer).toBeVisible();
      await page.waitForTimeout(1200);
      // Still this job, the drag still open (nothing was saved half-way).
      await expect(page.locator(".editor input.title")).toHaveValue(`Sledování ${kind}`);
      expect(shape(await loadProject(id))).toBe(shape(pre));
      await moveSteps(page, a, 6, 10);
      await page.mouse.up();
      const p = await waitForProject(id, moved(kind, pre), 6000);
      onlyTheDrop(kind, pre, p);
      if (kind === "corner") {
        // Where it was let go (ten steps), not where the pointer was when the AI called (five).
        const cut = await planXY(page, 0, 0);
        const end = await planXY(page, 1, 1);
        const perM = end.x - cut.x;
        expect(p.parts[0].edges[1].start.x - pre.parts[0].edges[1].start.x).toBeGreaterThan((a.step.x * 8) / perM);
      }
      await expect(page.locator(".editor input.title")).toHaveValue(`Sledování ${kind}`);
      await app.saved();
      await probe(app, page, id);
    });
  }

  test("leaving the job while a corner is dragged ends the drag where it is and saves it (TEST-601 G2)", async ({ app, page, mcp }) => {
    const { id, pre } = await stage(app, page, mcp, "Odchod při tahu");
    const a = await aim(page, "corner");
    await pressAt(page, a.at);
    await moveSteps(page, a, 1, 8);
    // Back is closed for the pointer while the drag is open; a leave that
    // does not come from the user's pointer (a file opened from the system,
    // the offer's Open) goes through the same guard as this click.
    await page.locator("header .icon-btn").first().dispatchEvent("click");
    await expect(page.locator(".editor")).toBeHidden({ timeout: 5000 });
    await page.mouse.up();
    const p = await waitForProject(id, moved("corner", pre), 3000);
    onlyTheDrop("corner", pre, p);
  });
});

test.describe("round-7 audit: another pointer during a touch drag does nothing, wherever it lands (UI-601, UI-605)", () => {
  test.use({ hasTouch: true });

  for (const kind of ["object", "building"] as const) {
    test(`a second finger on a dormer, a wall, a corner, the building and the background, and a mouse click on Delete and Undo, while a finger drags ${NAME[kind]}`, async ({ app, page, mcp }) => {
      const { id, pre } = await stage(app, page, mcp, `Dotyk ${kind}`);
      const a = await aim(page, kind);
      const spots: Record<string, XY> = {
        "another dormer": await dormerAt(page, 0),
        "a wall": await planXY(page, 6, 8),
        "a corner": await planXY(page, 0, 8),
        "the building": await planXY(page, 9, 6.5),
        "the background": await planXY(page, -1.2, 9.2),
      };
      const cdp = await page.context().newCDPSession(page);
      const touch = (type: string, pts: (XY & { id: number })[]) => cdp.send("Input.dispatchTouchEvent", { type, touchPoints: pts });
      let f1 = { ...a.at, id: 1 };
      await touch("touchStart", [f1]);
      for (let i = 1; i <= 8; i++) {
        f1 = { x: a.at.x + a.step.x * i, y: a.at.y + a.step.y * i, id: 1 };
        await touch("touchMove", [f1]);
        await page.waitForTimeout(30);
      }
      const selected = kind === "object" ? bar(page, "dormer.title").locator(".which strong") : page.locator("path.fill.movable");
      for (const [name, at] of Object.entries(spots)) {
        // Finger 2 lands, rests, moves and lifts; finger 1 keeps holding.
        await touch("touchStart", [f1, { ...at, id: 2 }]);
        await page.waitForTimeout(80);
        await touch("touchMove", [f1, { x: at.x + 12, y: at.y + 9, id: 2 }]);
        await page.waitForTimeout(40);
        // A move that no longer lists finger 2 lifts it (CDP sends one event per changed point).
        await touch("touchMove", [f1]);
        await page.waitForTimeout(40);
        // The selection is still the dragged thing.
        if (kind === "object") await expect(selected, name).toHaveText(cs("dormer.n", { n: 3 }));
        else await expect(selected, name).toHaveCount(1);
        await expect(page.getByTestId("plan-hint"), name).toHaveText(cs("plan.hintDrag"));
      }
      // The mouse (the other hand on the touchpad) clicks Delete / Undo: closed (ui t612 `del`, `undo`).
      await app.answerConfirm(true, true);
      for (const btn of [kind === "object" ? bar(page, "dormer.title").getByRole("button", { name: cs("dormer.delete") }) : tool(page, "tool.deletePart"), tool(page, "editor.undo")]) {
        const b = (await btn.boundingBox())!;
        await page.mouse.click(b.x + b.width / 2, b.y + b.height / 2);
      }
      expect(await app.dialogs()).toEqual([]);
      await expect(page.locator("g.dormer")).toHaveCount(3);
      // Finger 1 moves on and lifts: its drag is what is saved.
      for (let i = 9; i <= 10; i++) {
        f1 = { x: a.at.x + a.step.x * i, y: a.at.y + a.step.y * i, id: 1 };
        await touch("touchMove", [f1]);
        await page.waitForTimeout(30);
      }
      await touch("touchEnd", []);
      const dropped = await waitForProject(id, moved(kind, pre), 6000);
      await app.saved();
      onlyTheDrop(kind, pre, dropped);
      await page.waitForTimeout(600);
      expect(shape(await loadProject(id))).toBe(shape(dropped));
      await undoRedo(app, page, id, pre, dropped);
      await probe(app, page, id);
    });
  }

  test("a second finger that rests on the plan when the first lifts leaves no drag behind: a typed pitch is saved and Escape only deselects", async ({ app, page, mcp }) => {
    // ui t601 `line0` + ESC=1: the stuck drag held the autosave, and Escape
    // then rolled the job back to its start and saved that (UI-601, S1).
    const { id, pre } = await stage(app, page, mcp, "Dva prsty a Esc");
    const a = await aim(page, "building");
    const cdp = await page.context().newCDPSession(page);
    const touch = (type: string, pts: (XY & { id: number })[]) => cdp.send("Input.dispatchTouchEvent", { type, touchPoints: pts });
    const rest = await planXY(page, 9, 6.5);
    await touch("touchStart", [{ ...a.at, id: 1 }]);
    for (let i = 1; i <= 8; i++) {
      await touch("touchMove", [{ x: a.at.x + a.step.x * i, y: a.at.y, id: 1 }]);
      await page.waitForTimeout(30);
    }
    const f1 = { x: a.at.x + a.step.x * 8, y: a.at.y, id: 1 };
    await touch("touchStart", [f1, { ...rest, id: 2 }]);
    await page.waitForTimeout(150);
    // Finger 1 lifts first, finger 2 stays a moment and lifts without moving.
    await touch("touchMove", [{ ...rest, id: 2 }]);
    await page.waitForTimeout(150);
    await touch("touchEnd", []);
    const dropped = await waitForProject(id, moved("building", pre), 6000);
    onlyTheDrop("building", pre, dropped);
    await app.saved();
    // A pitch typed on the selected wall is saved by the autosave.
    const pitch = bar(page, "def.title").getByLabel(cs("edge.pitch"));
    await typeInto(page, pitch, "33");
    await page.keyboard.press("Enter");
    await waitForProject(id, (p) => p.parts[0].edges[0].pitch_deg === 33, 3000);
    await app.saved();
    // Escape deselects; it rolls nothing back.
    await page.locator("body").click({ position: { x: 5, y: 5 } }).catch(() => {});
    await page.keyboard.press("Escape");
    await page.waitForTimeout(1500);
    await app.saved();
    const end = await loadProject(id);
    expect(end.parts[0].edges[0].pitch_deg).toBe(33);
    expect(JSON.stringify(end.parts[0].edges.map((e: any) => e.start))).toBe(JSON.stringify(dropped.parts[0].edges.map((e: any) => e.start)));
  });
});

test.describe("round-8 audit: a building drop the core refuses is put back (TEST-701, ERR-701)", () => {
  for (const undo of [false, true]) {
    test(`a refused building drop puts the building back, names the move${undo ? ", and a Ctrl+Z pressed while it settles undoes nothing" : ""}, and is not saved with the next edit`, async ({ app, page, mcp }) => {
      const { id, pre } = await stage(app, page, mcp, `Odmítnuto ${undo ? "undo" : "plain"}`);
      const a = await aim(page, "building");
      const fill = page.locator("path.fill").first();
      const box0 = (await fill.boundingBox())!;
      let refuse = false;
      // The core refuses the move as a job over the input limits does (commands::transform_part).
      await page.route(/\/transform_part$/, async (r: Route) => {
        if (!refuse) return r.continue();
        await new Promise((res) => setTimeout(res, 1200));
        await r.fulfill({ contentType: "application/json", body: JSON.stringify({ err: { code: "out_of_range", detail: "a value is out of range (allowed ±100000 m)" } }) });
      });
      await pressAt(page, a.at);
      await moveSteps(page, a, 1, 9);
      refuse = true;
      await page.mouse.up();
      if (undo) {
        await page.waitForTimeout(250);
        await page.keyboard.press("ControlOrMeta+z");
      }
      // The refusal names the move, not a failed calculation.
      const toast = app.toasts().filter({ hasText: cs("err.action.move") });
      await expect(toast).toHaveCount(1, { timeout: 6000 });
      await expect(app.toasts().filter({ hasText: cs("err.action.calc") })).toHaveCount(0);
      // The building is back where it was pressed.
      await expect.poll(async () => Math.abs((await fill.boundingBox())!.x - box0.x)).toBeLessThan(3);
      await page.waitForTimeout(800);
      await app.saved();
      // Nothing saved, and the queued Undo did not take the step before the drag (dormer 1's width).
      expect(shape(await loadProject(id))).toBe(shape(pre));
      await expect(tool(page, "editor.redo")).toBeDisabled();
      // The next edit saves the job as it was before the drag, with the pitch.
      refuse = false;
      const pitch = bar(page, "def.title").getByLabel(cs("edge.pitch"));
      await typeInto(page, pitch, "33");
      await page.keyboard.press("Enter");
      const end = await waitForProject(id, (p) => p.parts[0].edges.some((e: any) => e.pitch_deg === 33), 3000);
      expect(JSON.stringify(end.parts[0].edges.map((e: any) => e.start))).toBe(JSON.stringify(pre.parts[0].edges.map((e: any) => e.start)));
      expect(JSON.stringify(end.dormers)).toBe(JSON.stringify(pre.dormers));
      // The refusal is logged for the developer console; not a page failure.
      const logged = app.errors.filter((e) => e.includes("[tectora] move"));
      expect(logged.length).toBe(1);
      app.errors.splice(0, app.errors.length, ...app.errors.filter((e) => !e.includes("[tectora] move")));
    });
  }
});
