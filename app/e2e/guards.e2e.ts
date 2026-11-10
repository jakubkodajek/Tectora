// Guards against losing or doubling work (review 2026-11-10): leaving waits
// for field handlers that ask the core (ERR-004), Accept / Revert run once
// (UI-002), Revert all asks (UX-011), a half-drawn footprint is not dropped
// silently (UX-013), no export or offer from a roof the core refused
// (ERR-003), the standard roof dialog adds the picked type (UI-001), browser
// ResizeObserver noise is no error (UI-019), Enter creates a new job (UX-017).
import type { Page, Route } from "@playwright/test";
import { bar, clickPlan, openEditor, selectWall, tool } from "./editor";
import { bridge, createExample, expect, listProjects, loadProject, test, waitForProject } from "./fixtures";
import { cs } from "./i18n";

/** Holds every call of a bridge command for `ms` before it goes through. */
const slow = (page: Page, cmd: string, ms: number) =>
  page.route(new RegExp(`/${cmd}$`), async (r: Route) => {
    await new Promise((res) => setTimeout(res, ms));
    await r.continue();
  });

const back = (page: Page) => page.getByRole("button", { name: cs("editor.back") });
const libraryRow = (page: Page, name: string) => page.getByRole("row").filter({ hasText: name }).first();

test.describe("leaving waits for a value still with the core (ERR-004)", () => {
  test("Back right after a window size is typed stores the size before the library shows", async ({ app, page }) => {
    const id = await createExample("gable", "Okno");
    const p = await loadProject(id);
    p.windows = [{ x: 4.5, y: 1, width: 0.78, height: 1.18 }];
    await bridge("save_project", { id, project: p });
    await openEditor(app, "Okno");
    await page.locator("path.opening.window").first().dispatchEvent("pointerdown", { button: 0 });
    const ob = bar(page, "opening.window");
    await slow(page, "check_windows", 800);
    const width = ob.getByLabel(cs("opening.width"));
    await width.click();
    await width.press("ControlOrMeta+a");
    await page.keyboard.type("1,14");
    await back(page).click();
    await expect(libraryRow(page, "Okno")).toBeVisible();
    // Read the moment the library shows: the editor is gone by now.
    expect((await loadProject(id)).windows[0].width).toBe(1.14);
  });

  test("Back right after the building height is typed stores it before the library shows", async ({ app, page }) => {
    const id = await createExample("gable", "Výška");
    await openEditor(app, "Výška");
    await selectWall(page, 0);
    await slow(page, "set_part_height", 800);
    const h = page.getByTestId("building-height");
    await h.click();
    await h.press("ControlOrMeta+a");
    await page.keyboard.type("4,2");
    await back(page).click();
    await expect(libraryRow(page, "Výška")).toBeVisible();
    const edges = (await loadProject(id)).parts[0].edges;
    expect(edges.every((e: any) => e.eave_height === 4.2)).toBe(true);
  });
});

const review = (page: Page) => page.locator(".review");
const pitches = async (id: string) => (await loadProject(id)).parts[0].edges.map((e: any) => e.pitch_deg).join(",");

test.describe("Accept and Revert run once (UI-002)", () => {
  test("a double-click on Revert in the AI tab reverts only the latest AI step", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "Dvojklik");
    await openEditor(app, "Dvojklik");
    await mcp.call("update_edges", { id, changes: [{ index: 0, pitch_deg: 25 }], summary: "Krok A" });
    await expect(review(page)).toContainText("Krok A");
    await mcp.call("update_edges", { id, changes: [{ index: 2, pitch_deg: 26 }], summary: "Krok B" });
    await expect(review(page)).toContainText(cs("review.steps", { n: 2 }));
    await page.locator("aside .tabs").getByRole("tab", { name: cs("dock.ai") }).click();
    await slow(page, "revert_last_change", 400);
    await page.locator("aside .panel").getByRole("button", { name: cs("ai.revert"), exact: true }).first().dblclick();
    await waitForProject(id, (p) => p.parts[0].edges[2].pitch_deg === 40);
    await page.waitForTimeout(1500);
    expect(await pitches(id)).toBe("25,90,40,90");
  });

  test("a double-click on Accept accepts once and leaves no error", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "Přijmout");
    await openEditor(app, "Přijmout");
    await mcp.call("update_edges", { id, changes: [{ index: 0, pitch_deg: 25 }], summary: "Krok A" });
    await expect(review(page)).toBeVisible();
    await review(page).getByRole("button", { name: cs("review.accept") }).dblclick();
    await expect(review(page)).toHaveCount(0);
    expect(await pitches(id)).toBe("25,90,40,90");
    await expect(app.toasts()).toHaveCount(0);
  });
});

test.describe("Revert all asks first (UX-011)", () => {
  test("Revert all names the AI steps and keeps them when the user says no", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "Vše");
    await openEditor(app, "Vše");
    await mcp.call("update_edges", { id, changes: [{ index: 0, pitch_deg: 30 }], summary: "Krok 1" });
    await expect(review(page)).toContainText("Krok 1");
    await mcp.call("update_edges", { id, changes: [{ index: 2, pitch_deg: 26 }], summary: "Krok 2" });
    await mcp.call("update_job", { id, customer: "AI Zákazník", summary: "Krok 3" });
    await expect(review(page)).toContainText(cs("review.steps", { n: 3 }));
    await app.answerConfirm(false);
    await review(page).getByRole("button", { name: cs("review.revertAll") }).click();
    await expect.poll(async () => (await app.dialogs()).map((d) => d.message)).toEqual([cs("review.confirmRevertAll", { n: 3 })]);
    await page.waitForTimeout(800);
    expect(await pitches(id)).toBe("30,90,26,90");
    await expect(review(page)).toContainText(cs("review.steps", { n: 3 }));
    await app.answerConfirm(true);
    await review(page).getByRole("button", { name: cs("review.revertAll") }).click();
    const p = await waitForProject(id, (p) => p.parts[0].edges[0].pitch_deg === 40 && p.parts[0].edges[2].pitch_deg === 40);
    expect(p.job.customer).toBe("");
    await expect(review(page)).toHaveCount(0);
  });

  test("Revert of a single AI step from the banner does not ask", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "Jeden");
    await openEditor(app, "Jeden");
    await mcp.call("update_edges", { id, changes: [{ index: 0, pitch_deg: 30 }], summary: "Krok 1" });
    await expect(review(page)).toContainText("Krok 1");
    await review(page).getByRole("button", { name: cs("ai.revert"), exact: true }).click();
    await waitForProject(id, (p) => p.parts[0].edges[0].pitch_deg === 40);
    expect(await app.dialogs()).toEqual([]);
  });
});
