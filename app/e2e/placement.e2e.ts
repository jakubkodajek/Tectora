// Distances on the roof, the Properties dialog, building height vs eave
// height and one bad building next to a good one (Jakub 2026-11-14; review
// TEST-001). The gable example is 12 x 8 m, 40°, 0.5 m overhang: its front
// face runs from the eave line at y = -0.5 up to the ridge at y = 4.
import type { Page } from "@playwright/test";
import { bar, clickPlan, openEditor, planXY, selectWall, setField, shownArea, tool } from "./editor";
import { bridge, createExample, expect, loadProject, test, waitForProject } from "./fixtures";
import { cs } from "./i18n";

async function withItems(id: string, edit: (p: any) => void) {
  const p = await loadProject(id);
  edit(p);
  await bridge("save_project", { id, project: p });
}

const placement = async (id: string, kind: string, index: number) =>
  bridge<any>("item_placement", { project: await loadProject(id), kind, index });

async function doubleClickPlan(page: Page, x: number, y: number) {
  const p = await planXY(page, x, y);
  await page.mouse.dblclick(p.x, p.y);
}

test.describe("distances on the roof", () => {
  test("typed distances move a window there, two typed quickly both land", async ({ app, page }) => {
    const id = await createExample("gable", "Sedlo");
    await withItems(id, (p) => (p.windows = [{ x: 3, y: 1.5, width: 0.78, height: 1.18 }]));
    await openEditor(app, "Sedlo");
    await clickPlan(page, 3.4, 1.9);
    const ob = bar(page, "opening.window");
    await expect(ob.getByTestId("place-left")).toHaveValue("3,500");
    // The plan shows the dimension lines of the selected window.
    await expect(page.getByTestId("place-dims")).toBeVisible();
    // Two fields in a row without waiting (review UI-002).
    await ob.getByTestId("place-eave").fill("2");
    await ob.getByTestId("place-eave").press("Tab");
    await ob.getByTestId("place-left").fill("1,5");
    await ob.getByTestId("place-left").press("Enter");
    await expect
      .poll(async () => {
        const pl = await placement(id, "window", 0);
        return [+pl.from_eave.toFixed(3), +pl.from_left.toFixed(3)];
      })
      .toEqual([2, 1.5]);
    await expect(ob.getByTestId("place-eave")).toHaveValue("2,000");
  });

  test("a distance past the ridge is refused and the window stays", async ({ app, page }) => {
    const id = await createExample("gable", "Sedlo");
    await withItems(id, (p) => (p.windows = [{ x: 3, y: 1.5, width: 0.78, height: 1.18 }]));
    await openEditor(app, "Sedlo");
    await clickPlan(page, 3.4, 1.9);
    const ob = bar(page, "opening.window");
    const before = (await loadProject(id)).windows[0];
    await setField(ob.getByTestId("place-eave"), "150");
    await expect(page.getByText(cs("place.refusedTitle"))).toBeVisible();
    // The field shows where the window still is.
    await expect(ob.getByTestId("place-eave")).not.toHaveValue("150,000");
    await page.waitForTimeout(500);
    expect((await loadProject(id)).windows[0]).toEqual(before);
    // The fields stay, so the value can be typed again (review UI-006).
    await expect(ob.getByTestId("place-eave")).toBeVisible();
    // The refusal is reported (toast and log), as intended.
    app.errors.length = 0;
  });
});

test.describe("distances by arrow keys", () => {
  test("four quick arrow steps move a chimney four steps", async ({ app, page }) => {
    const id = await createExample("gable", "Sedlo");
    await withItems(id, (p) => (p.chimneys = [{ corners: [{ x: 8, y: 5.5 }, { x: 8.6, y: 5.5 }, { x: 8.6, y: 6.1 }, { x: 8, y: 6.1 }], height: 0 }]));
    await openEditor(app, "Sedlo");
    await clickPlan(page, 8.3, 5.8);
    const ob = bar(page, "opening.chimney");
    const left = ob.getByTestId("place-left");
    await expect(left).toBeVisible();
    const start = (await placement(id, "chimney", 0)).from_left;
    await left.focus();
    for (let i = 0; i < 4; i++) await page.keyboard.press("ArrowUp");
    // Review UI-002: each step went from the value not yet back from the core.
    await expect.poll(async () => +((await placement(id, "chimney", 0)).from_left - start).toFixed(3)).toBe(0.2);
  });
});

test.describe("Properties", () => {
  test("a double-click on a chimney opens its dialog; Cancel undoes, OK keeps", async ({ app, page }) => {
    const id = await createExample("gable", "Sedlo");
    await withItems(id, (p) => (p.chimneys = [{ corners: [{ x: 8, y: 5.5 }, { x: 8.6, y: 5.5 }, { x: 8.6, y: 6.1 }, { x: 8, y: 6.1 }], height: 0 }]));
    await openEditor(app, "Sedlo");
    const before = (await loadProject(id)).chimneys[0];
    await doubleClickPlan(page, 8.3, 5.8);
    const dlg = page.getByTestId("object-dialog");
    await expect(dlg).toBeVisible();
    await setField(dlg.getByLabel(cs("opening.top")), 9);
    await waitForProject(id, (p) => p.chimneys[0].height === 9);
    await page.getByRole("button", { name: cs("common.cancel"), exact: true }).click();
    await waitForProject(id, (p) => p.chimneys[0].height === before.height);
    // Undo does not bring the cancelled edit back.
    await page.keyboard.press("Control+z");
    await page.waitForTimeout(500);
    expect((await loadProject(id)).chimneys[0].height).not.toBe(9);
    await doubleClickPlan(page, 8.3, 5.8);
    await setField(page.getByTestId("object-dialog").getByLabel(cs("opening.top")), 9);
    await page.getByTestId("object-dialog-ok").click();
    await page.waitForTimeout(500);
    expect((await loadProject(id)).chimneys[0].height).toBe(9);
  });
});

test.describe("building height and eave height", () => {
  test("an eave set below the building height keeps its offset when the building moves", async ({ app, page }) => {
    const id = await createExample("gable", "Sedlo");
    await openEditor(app, "Sedlo");
    await selectWall(page, 0);
    const def = bar(page, "def.title");
    if (!(await def.getByTestId("eave-height").isVisible())) await page.getByTestId("def-more").click();
    await setField(def.getByTestId("eave-height"), "2,6");
    let p = await waitForProject(id, (p) => p.parts[0].edges[0].eave_height === 2.6);
    // The building keeps the height it showed (3 m).
    expect(p.parts[0].height).toBe(3);
    await expect(def.getByTestId("building-height")).toHaveValue("3");
    await setField(def.getByTestId("building-height"), "4");
    p = await waitForProject(id, (p) => p.parts[0].height === 4);
    expect(p.parts[0].edges.map((e: any) => e.eave_height)).toEqual([3.6, 4, 4, 4]);
  });
});

test.describe("one bad building", () => {
  test("a wall shorter than 1 cm leaves only its building out; export waits", async ({ app, page }) => {
    const id = await createExample("gable", "Sedlo");
    await withItems(id, (p) => {
      const e = (x: number, y: number) => ({ ...p.parts[0].edges[0], start: { x, y }, pitch_deg: 35 });
      p.parts.push({ name: "Křídlo", edges: [e(20, 0), e(26, 0), e(26, 0.004), e(26, 6), e(20, 6)] });
    });
    await openEditor(app, "Sedlo");
    // The first building keeps its roof.
    await expect.poll(() => shownArea(page)).toBeGreaterThan(100);
    await expect(page.getByText(cs("prob.title", { n: 1 }))).toBeVisible();
    await expect(tool(page, "tool.export")).toBeDisabled();
    const r = await bridge<any>("calc", { project: await loadProject(id) });
    expect(r.parts[0].geometry.error ?? null).toBeNull();
    expect(r.problems[0].code).toBe("zero_length_wall");
  });
});
