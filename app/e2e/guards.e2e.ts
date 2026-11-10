// Guards against losing or doubling work (review 2026-11-10): leaving waits
// for field handlers that ask the core (ERR-004), Accept / Revert run once
// (UI-002), Revert all asks (UX-011), a half-drawn footprint is not dropped
// silently (UX-013), no export or offer from a roof the core refused
// (ERR-003), the standard roof dialog adds the picked type (UI-001), browser
// ResizeObserver noise is no error (UI-019), Enter creates a new job (UX-017).
import type { Page, Route } from "@playwright/test";
import { bar, clickPlan, openEditor, selectWall, tool } from "./editor";
import { bridge, createExample, expect, listProjects, loadProject, test } from "./fixtures";
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
