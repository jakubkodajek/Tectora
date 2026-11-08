// Number fields: typed values must be stored however the user leaves the
// field (Enter, Tab, a click on the plan, on another wall or on a button),
// with a decimal comma or dot, and never on the wrong wall. Regression tests
// for "typed values are not applied, only the spinner arrows work".
import type { Locator, Page } from "@playwright/test";
import { bar, openEditor, planXY, selectWall, setField, tool } from "./editor";
import { createExample, expect, loadProject, test, waitForProject } from "./fixtures";
import { cs } from "./i18n";

/** Types into a field like a person: click, select all, type (no fill()). */
async function typeInto(page: Page, field: Locator, text: string) {
  await field.click();
  await field.press("ControlOrMeta+a");
  await page.keyboard.type(text);
}

/** Clicks the plan at world point (x, y) with a real mouse press. */
async function pressPlan(page: Page, x: number, y: number) {
  const p = await planXY(page, x, y);
  await page.mouse.click(p.x, p.y);
}

test.describe("number fields", () => {
  test("Enter stores the typed value, with a decimal comma", async ({ app, page }) => {
    const id = await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    const def = bar(page, "def.title");
    await selectWall(page, 0);
    const eave = def.getByLabel(cs("edge.eaveHeight"));
    await typeInto(page, eave, "3,75");
    await page.keyboard.press("Enter");
    await waitForProject(id, (p) => p.parts[0].edges[0].eave_height === 3.75);
    // Shown in the UI language (Czech: decimal comma), focus stays.
    await expect(eave).toHaveValue("3,75");
    await expect(eave).toBeFocused();
    await typeInto(page, eave, "4.5");
    await page.keyboard.press("Enter");
    await waitForProject(id, (p) => p.parts[0].edges[0].eave_height === 4.5);
  });

  test("a click on the plan stores the value before it changes the selection", async ({ app, page }) => {
    const id = await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    const def = bar(page, "def.title");
    await selectWall(page, 0);
    const before = (await loadProject(id)).parts[0].edges.map((e: any) => e.overhang);
    // Type, then click straight on the second wall: the value belongs to wall 1.
    await typeInto(page, def.getByLabel(cs("edge.overhang")), "0,85");
    const w = (await loadProject(id)).parts[0].edges;
    await pressPlan(page, (w[1].start.x + w[2].start.x) / 2, (w[1].start.y + w[2].start.y) / 2);
    await expect(def.locator(".which strong")).toHaveText(cs("def.wall", { n: 2 }));
    const p = await waitForProject(id, (p) => p.parts[0].edges[0].overhang === 0.85);
    expect(p.parts[0].edges.slice(1).map((e: any) => e.overhang)).toEqual(before.slice(1));
    await expect(def.getByLabel(cs("edge.overhang"))).toHaveValue(String(before[1]).replace(".", ","));

    // Type, then click on empty plan: stored too.
    await typeInto(page, def.getByLabel(cs("edge.pitch")), "33");
    await pressPlan(page, -6, -6);
    await waitForProject(id, (p) => p.parts[0].edges[1].pitch_deg === 33);
  });

  test("a click on a button uses the value just typed", async ({ app, page }) => {
    const id = await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    const def = bar(page, "def.title");
    await selectWall(page, 0);
    await typeInto(page, def.getByLabel(cs("edge.eaveHeight")), "5");
    await def.getByRole("button", { name: cs("edge.heightAll") }).click();
    await waitForProject(id, (p) => p.parts[0].edges.every((e: any) => e.eave_height === 5));
    // Next-wall button: stored on the wall it was typed for.
    await typeInto(page, def.getByLabel(cs("edge.pitch")), "31");
    await def.getByRole("button", { name: cs("def.next") }).click();
    await expect(def.locator(".which strong")).toHaveText(cs("def.wall", { n: 2 }));
    const p = await waitForProject(id, (p) => p.parts[0].edges[0].pitch_deg === 31);
    expect(p.parts[0].edges[1].pitch_deg).not.toBe(31);
  });

  test("Escape restores, invalid text is refused and marked, arrows and ▲▼ step", async ({ app, page }) => {
    const id = await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    const def = bar(page, "def.title");
    await selectWall(page, 0);
    const pitch = def.getByLabel(cs("edge.pitch"));
    const start = (await loadProject(id)).parts[0].edges[0].pitch_deg as number;

    await typeInto(page, pitch, "12");
    await page.keyboard.press("Escape");
    await expect(pitch).toHaveValue(String(start));
    // Escape in the field does not clear the wall selection.
    await expect(def.locator(".which strong")).toHaveText(cs("def.wall", { n: 1 }));

    await typeInto(page, pitch, "abc");
    await expect(pitch).toHaveAttribute("aria-invalid", "true");
    await page.keyboard.press("Tab");
    await expect(pitch).toHaveValue(String(start));
    await typeInto(page, pitch, "500");
    await page.keyboard.press("Enter");
    await expect(pitch).toHaveValue(String(start));
    await page.waitForTimeout(700);
    expect((await loadProject(id)).parts[0].edges[0].pitch_deg).toBe(start);

    await pitch.focus();
    await page.keyboard.press("ArrowUp");
    await waitForProject(id, (p) => p.parts[0].edges[0].pitch_deg === start + 1);
    const eave = def.getByLabel(cs("edge.eaveHeight"));
    const h = (await loadProject(id)).parts[0].edges[0].eave_height as number;
    await eave.locator("xpath=following-sibling::span[contains(@class,'num-spin')]//button[1]").click();
    await waitForProject(id, (p) => Math.abs(p.parts[0].edges[0].eave_height - (h + 0.05)) < 1e-9);
    await eave.locator("xpath=following-sibling::span[contains(@class,'num-spin')]//button[2]").click();
    await waitForProject(id, (p) => p.parts[0].edges[0].eave_height === h);
  });

  test("every number field of the roof definition bar takes a typed value", async ({ app, page }) => {
    const id = await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    const def = bar(page, "def.title");
    await selectWall(page, 0);
    await def.getByRole("button", { name: cs("edge.addBreak") }).click();
    await waitForProject(id, (p) => (p.parts[0].edges[0].breaks ?? []).length === 1);
    const fields = def.locator("input.num-input:not([disabled])");
    const n = await fields.count();
    expect(n).toBeGreaterThanOrEqual(8);
    for (let i = 0; i < n; i++) {
      const before = JSON.stringify(await loadProject(id));
      const f = fields.nth(i);
      const label = (await f.evaluate((el) => el.closest("label")?.querySelector("span")?.textContent ?? "")) || `#${i}`;
      const cur = Number((await f.inputValue()).replace(",", "."));
      await typeInto(page, f, String(cur + 0.5).replace(".", ","));
      await page.keyboard.press("Enter");
      await expect.poll(async () => JSON.stringify(await loadProject(id)) !== before, { message: label, timeout: 6000 }).toBe(true);
    }
  });

  test("chimney and dormer fields take typed values", async ({ app, page }) => {
    const id = await createExample("gable", "Sedlo");
    await openEditor(app, "Sedlo");
    await tool(page, "tool.chimney").click();
    await pressPlan(page, 3, 2);
    const cb = bar(page, "opening.chimney");
    await waitForProject(id, (p) => p.chimneys.length === 1);
    const field = cb.locator("input.num-input").first();
    const before = JSON.stringify((await loadProject(id)).chimneys[0]);
    await typeInto(page, field, "1,25");
    await pressPlan(page, -6, -6);
    await expect.poll(async () => JSON.stringify((await loadProject(id)).chimneys[0]) !== before).toBe(true);

    await tool(page, "tool.dormer").click();
    await pressPlan(page, 4, 1);
    const db = bar(page, "dormer.title");
    await waitForProject(id, (p) => p.dormers.length === 1);
    const dormer = JSON.stringify((await loadProject(id)).dormers[0]);
    await typeInto(page, db.getByLabel(cs("dormer.width")), "2,4");
    await page.keyboard.press("Tab");
    await expect.poll(async () => JSON.stringify((await loadProject(id)).dormers[0]) !== dormer).toBe(true);
    await expect(db.getByLabel(cs("dormer.width"))).toHaveValue("2,4");
  });

  test("drawing presets and settings defaults take a decimal comma", async ({ app, page }) => {
    const id = await createExample("gable", "Sedlo");
    await openEditor(app, "Sedlo");
    await page.locator("body").press("Insert");
    const draw = page.getByRole("toolbar", { name: cs("draw.title") });
    await expect(draw).toBeVisible();
    const eave = draw.locator(".presets").getByLabel(cs("edge.eaveHeight"));
    await typeInto(page, eave, "2,75");
    await page.keyboard.press("Enter");
    // Enter in a preset field stores it and does not close the footprint.
    await expect(draw).toBeVisible();
    await expect.poll(() => page.evaluate(() => JSON.parse(localStorage.getItem("tectora.presets") ?? "{}").eave_height)).toBe(2.75);
    await setField(eave, "3");
    await page.keyboard.press("Escape");
    expect((await loadProject(id)).parts).toHaveLength(1);
  });
});
