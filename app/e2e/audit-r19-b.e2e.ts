// Guards for the round-19 audit (2026-11-24), editor side, second half.
//
// UI-1801  after Revert, Revert all, the AI tab's Revert, Undo / Redo through
//          the core and the offer put-back the plan and the 3D view take a
//          real mouse click (no `dispatchEvent`: it reaches inert elements).
import type { Page } from "@playwright/test";
import { bar, openEditor } from "./editor";
import { createExample, expect, test, waitForProject } from "./fixtures";
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
