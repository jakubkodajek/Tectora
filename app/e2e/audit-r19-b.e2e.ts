// Guards for the round-19 audit (2026-11-24), editor side, second half.
//
// HUNT-1801 a hand-drawn face clicked out of order (a bow tie) is refused
//          with a note and the draft stays open; in order it is added.
// UI-1801  after Revert, Revert all, the AI tab's Revert, Undo / Redo through
//          the core and the offer put-back the plan and the 3D view take a
//          real mouse click (no `dispatchEvent`: it reaches inert elements).
import fs from "node:fs";
import path from "node:path";
import type { Page } from "@playwright/test";
import { bar, clickPlan, openEditor, tool } from "./editor";
import { LIBRARY_DIR } from "./env";
import { createExample, expect, loadProject, test, waitForProject } from "./fixtures";
import { cs } from "./i18n";
import { olderOffer } from "./review";

// TECTORA_SHOTS=<folder>/ saves a screenshot after each test (fix evidence).
test.afterEach(async ({ page }, info) => {
  const dir = process.env.TECTORA_SHOTS;
  if (!dir) return;
  const id = info.title.match(/(HUNT|UX|UI|ERR|TEST|ARCH)-\d+/)?.[0] ?? "r19";
  const n = info.title.replace(/[^a-z0-9]+/gi, "-").slice(0, 40);
  await page.screenshot({ path: `${dir}r19-${id}-${n}-${process.env.TECTORA_SHOTS_TAG ?? "after"}.png`, timeout: 10_000 }).catch(() => {});
});

const review = (page: Page) => page.locator(".review");
const dockTab = (page: Page, key: string) => page.locator("aside .tabs").getByRole("tab", { name: cs(key) });
const footerSave = (page: Page) => page.locator(".statusbar .save");
const undo = (page: Page) => page.locator("body").press("ControlOrMeta+z");
const redo = (page: Page) => page.locator("body").press("ControlOrMeta+Shift+z");

/** The point of the page in the middle of `loc`, and nothing covers it. */
async function freePoint(page: Page, loc: import("@playwright/test").Locator) {
  const box = await loc.boundingBox();
  expect(box, "the element is on the page").not.toBeNull();
  const x = box!.x + box!.width / 2;
  const y = box!.y + box!.height / 2;
  // `inert` does not change what lies on top, but an inert element is not hit-tested:
  // Chromium returns its live ancestor (UI-1801 read "<div class=main> intercepts").
  const hit = await loc.evaluate((el, [px, py]) => {
    const top = document.elementFromPoint(px, py);
    return { mine: !!top && (top === el || el.contains(top)), inert: !!el.closest("[inert]") };
  }, [x, y] as const);
  expect(hit, "the element itself takes the click").toEqual({ mine: true, inert: false });
  return { x, y };
}

/** The editor is live again: a real click on wall `index` selects it, a real click on a 3D view button picks it. */
async function editorIsLive(page: Page, index: number) {
  await expect(footerSave(page)).toHaveText(cs("editor.saved"));
  const hit = page.locator(".view .wrap > svg g.edge line.hit").nth(index);
  const p = await freePoint(page, hit);
  await page.mouse.click(p.x, p.y);
  await expect(bar(page, "def.title").locator(".which strong")).toHaveText(cs("def.wall", { n: index + 1 }));
  const views = page.getByRole("radiogroup", { name: cs("view.title") });
  const view = views.getByRole("radio").nth(index + 1);
  const v = await freePoint(page, view);
  await page.mouse.click(v.x, v.y);
  await expect(view).toHaveAttribute("aria-checked", "true");
}

async function oneAiSave(page: Page, mcp: any, id: string) {
  await mcp.call("update_edges", { id, changes: [{ index: 0, overhang: 0.9 }], summary: "AI přesah 0.9" });
  await expect(review(page)).toContainText("AI přesah 0.9");
}

test.describe("round 19, UI-1801: the plan and the 3D view are live after every review transaction", () => {
  test("after the banner's Revert a real click selects a wall and picks a 3D view (UI-1801)", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "R19 živý revert");
    await openEditor(app, "R19 živý revert");
    await oneAiSave(page, mcp, id);
    await review(page).getByRole("button", { name: cs("ai.revert"), exact: true }).click();
    await waitForProject(id, (p) => p.parts[0].edges[0].overhang !== 0.9);
    await expect(review(page)).toHaveCount(0);
    await editorIsLive(page, 1);
    await editorIsLive(page, 0);
  });

  test("after Revert all a real click selects a wall (UI-1801)", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "R19 živý vše");
    await openEditor(app, "R19 živý vše");
    await oneAiSave(page, mcp, id);
    await mcp.call("update_job", { id, notes: "Od AI", summary: "Poznámka od AI" });
    await expect(review(page)).toContainText(cs("review.steps", { n: 2 }));
    await app.answerConfirm(true);
    await review(page).getByRole("button", { name: cs("review.revertAll") }).click();
    await waitForProject(id, (p) => !p.notes && p.parts[0].edges[0].overhang !== 0.9);
    await expect(review(page)).toHaveCount(0);
    await editorIsLive(page, 1);
  });

  test("after the AI tab's Revert a real click selects a wall (UI-1801)", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "R19 živý panel");
    await openEditor(app, "R19 živý panel");
    await oneAiSave(page, mcp, id);
    await dockTab(page, "dock.ai").click();
    await page.getByRole("tabpanel").getByRole("button", { name: cs("ai.revert"), exact: true }).click();
    await waitForProject(id, (p) => p.parts[0].edges[0].overhang !== 0.9);
    await expect(review(page)).toHaveCount(0);
    await editorIsLive(page, 1);
  });

  test("after Ctrl+Z over an AI save and after Ctrl+Y a real click selects a wall (UI-1801)", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "R19 živý zpět");
    await openEditor(app, "R19 živý zpět");
    await oneAiSave(page, mcp, id);
    await undo(page);
    await waitForProject(id, (p) => p.parts[0].edges[0].overhang !== 0.9);
    await expect(review(page)).toHaveCount(0);
    await editorIsLive(page, 1);
    await page.locator("body").focus();
    await redo(page);
    await waitForProject(id, (p) => p.parts[0].edges[0].overhang === 0.9);
    await expect(review(page)).toContainText("AI přesah 0.9");
    await editorIsLive(page, 0);
  });

  test("after the put-back of the offer's figures a real click selects a wall (UI-1801)", async ({ app, page, mcp }) => {
    const id = await olderOffer(mcp, "R19 živý nabídka");
    await app.open({ prefs: { followAi: false } });
    await app.openJob("R19 živý nabídka");
    await waitForProject(id, (q) => q.result_snapshot?.engine_version !== "0.8.0");
    await app.saved();
    await dockTab(page, "dock.results").click();
    await page.getByTestId("snap-restore").click();
    await waitForProject(id, (q) => q.result_snapshot?.engine_version === "0.8.0");
    await expect(page.getByTestId("snap-restore")).toHaveCount(0);
    await editorIsLive(page, 1);
  });
});

test.describe("round 19, HUNT-1801: a hand-drawn face that crosses itself", () => {
  test("four corners clicked out of order and Enter: a note, no face, the draft stays; in order it closes (HUNT-1801)", async ({ app, page }) => {
    const id = await createExample("gable", "R19 motýlek");
    await openEditor(app, "R19 motýlek");
    await app.saved();
    await tool(page, "tool.face").click();
    const pb = page.getByTestId("place-bar");
    for (const [x, y] of [
      [2, 1],
      [6, 3],
      [6, 1],
      [2, 3],
    ])
      await clickPlan(page, x, y);
    await expect(pb.getByTestId("place-points")).toHaveText(cs("place.points", { n: 4 }));
    await page.keyboard.press("Enter");
    await expect(app.toasts().filter({ hasText: cs("manual.refused.face_crosses") })).toHaveCount(1);
    await expect(pb).toBeVisible();
    await expect(pb.getByTestId("place-points")).toHaveText(cs("place.points", { n: 4 }));
    expect((await loadProject(id)).manual_faces ?? []).toHaveLength(0);
    // Esc and the same corners in order: the face is added, no note.
    await page.keyboard.press("Escape");
    await expect(pb).toHaveCount(0);
    await tool(page, "tool.face").click();
    for (const [x, y] of [
      [2, 1],
      [6, 1],
      [6, 3],
      [2, 3],
    ])
      await clickPlan(page, x, y);
    await page.keyboard.press("Enter");
    await waitForProject(id, (p) => (p.manual_faces ?? []).length === 1);
    await expect(pb).toHaveCount(0);
  });

  test("a stored face that crosses itself keeps its area and the Results tab warns about it (HUNT-1801)", async ({ app, page }) => {
    const id = await createExample("gable", "R19 uložený motýlek");
    const p = await loadProject(id);
    p.manual_faces = [{ points: [{ x: 1, y: 1 }, { x: 6, y: 6 }, { x: 6, y: 1 }, { x: 1, y: 6 }], pitch_deg: 30 }];
    fs.writeFileSync(path.join(LIBRARY_DIR, `${id}.tectora`), JSON.stringify(p));
    await openEditor(app, "R19 uložený motýlek");
    await dockTab(page, "dock.results").click();
    await expect(page.getByTestId("results-warned")).toBeVisible();
    await expect(page.getByRole("tabpanel")).toContainText(cs("res.warn.manual_face_crosses", { n: 1 }));
  });
});

test.describe("round 19, UX-1802: Undo / Redo pressed in the dock while an Undo is with the core", () => {
  test("two Ctrl+Z with the focus on the AI tab: the second is queued behind the first and both go out (UX-1802)", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "R19 fronta v panelu");
    await openEditor(app, "R19 fronta v panelu");
    await mcp.call("update_job", { id, notes: "Od AI", summary: "Poznámka od AI" });
    await expect(review(page)).toContainText("Poznámka od AI");
    await mcp.call("update_job", { id, customer: "AI Zákazník", summary: "Zákazník od AI" });
    await expect(review(page)).toContainText(cs("review.steps", { n: 2 }));
    await app.saved();
    await dockTab(page, "dock.ai").click();
    await dockTab(page, "dock.ai").focus();
    let calls = 0;
    let open!: () => void;
    const gate = new Promise<void>((r) => (open = r));
    await page.route("**/revert_changes", async (route) => {
      calls++;
      const res = await route.fetch();
      if (calls === 1) await gate;
      await route.fulfill({ response: res }).catch(() => {});
    });
    await page.keyboard.press("ControlOrMeta+z");
    await expect.poll(() => calls).toBe(1);
    // The focus is still in the dock while the first answer is held.
    expect(await page.evaluate(() => !!document.activeElement?.closest("aside"))).toBe(true);
    await page.keyboard.press("ControlOrMeta+z");
    await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => r(null))));
    expect(calls).toBe(1);
    open();
    await waitForProject(id, (p) => !p.notes && p.job.customer === "");
    await expect.poll(() => calls).toBe(2);
    await expect(review(page)).toHaveCount(0);
    expect(await page.locator(".stack .toast.error").count()).toBe(0);
  });
});

test.describe("round 19, UX-1803 / UI-1803: the status bar during an Undo / Redo through the core", () => {
  test("Ctrl+Z says undoing and Ctrl+Shift+Z says redoing while the core answers, not reverting (UX-1803)", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "R19 stav zpět");
    await openEditor(app, "R19 stav zpět");
    await oneAiSave(page, mcp, id);
    await app.saved();
    for (const [key, cmd, text, check] of [
      [undo, "revert_changes", "editor.undoing", (p: any) => p.parts[0].edges[0].overhang !== 0.9],
      [redo, "take_back_revert", "editor.redoing", (p: any) => p.parts[0].edges[0].overhang === 0.9],
    ] as const) {
      let open!: () => void;
      const gate = new Promise<void>((r) => (open = r));
      let held = 0;
      await page.route(`**/${cmd}`, async (route) => {
        held++;
        const res = await route.fetch();
        await gate;
        await route.fulfill({ response: res }).catch(() => {});
      });
      await key(page);
      await expect.poll(() => held).toBe(1);
      await expect(footerSave(page)).toHaveText(cs(text));
      await expect(footerSave(page)).not.toHaveText(cs("editor.reverting"));
      open();
      await waitForProject(id, check);
      await expect(footerSave(page)).toHaveText(cs("editor.saved"));
      await page.unroute(`**/${cmd}`);
    }
  });
});
