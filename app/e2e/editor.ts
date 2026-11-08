// Editor helpers: toolbar buttons, the bars under the views, and clicking the
// plan at world coordinates (calibrated from the status bar's cursor read-out).
import type { Locator, Page } from "@playwright/test";
import { expect, type App } from "./fixtures";
import { cs } from "./i18n";

export const plan = (page: Page) => page.locator(".view .wrap > svg").first();

/** A toolbar button by its translated label key (exact match). */
export const tool = (page: Page, key: string) =>
  page.getByRole("toolbar").first().getByRole("button", { name: cs(key), exact: true });

/** The bar under the views (roof definition, dormer, opening, …). */
export const bar = (page: Page, key: string) => page.getByRole("toolbar", { name: cs(key) });

/** Types a value into a field and commits it (the app listens to `change`). */
export async function setField(field: Locator, value: string | number) {
  await field.fill(String(value));
  await field.press("Tab");
}

function parseCoord(s: string): number {
  return Number(s.replace(/−/g, "-").replace(/\s/g, "").replace(",", "."));
}

async function cursorAt(page: Page, px: number, py: number): Promise<{ x: number; y: number }> {
  await page.mouse.move(px, py);
  const text = (await page.locator(".statusbar .coords").textContent()) ?? "";
  const m = text.match(/X\s*([-−\d\s,.]+?)\s+Y\s*([-−\d\s,.]+)/);
  if (!m) throw new Error(`no cursor coordinates in the status bar: ${text}`);
  return { x: parseCoord(m[1]), y: parseCoord(m[2]) };
}

/** Screen position of world point (x, y) in the plan, in page pixels. */
export async function planXY(page: Page, x: number, y: number): Promise<{ x: number; y: number }> {
  const box = (await plan(page).boundingBox())!;
  const a = { x: box.x + box.width * 0.3, y: box.y + box.height * 0.3 };
  const b = { x: box.x + box.width * 0.7, y: box.y + box.height * 0.7 };
  // Alt turns the 5 cm snap off, so the read-out is exact to 1 mm.
  await page.keyboard.down("Alt");
  const wa = await cursorAt(page, a.x, a.y);
  const wb = await cursorAt(page, b.x, b.y);
  await page.keyboard.up("Alt");
  const kx = (b.x - a.x) / (wb.x - wa.x);
  const ky = (b.y - a.y) / (wb.y - wa.y);
  return { x: a.x + (x - wa.x) * kx, y: a.y + (y - wa.y) * ky };
}

/** Clicks the plan at world point (x, y); the app snaps it to 5 cm. */
export async function clickPlan(page: Page, x: number, y: number) {
  const p = await planXY(page, x, y);
  await page.mouse.click(p.x, p.y);
}

/** Selects wall `index` by its hit line; walls are numbered part by part, in edge order. */
export async function selectWall(page: Page, index: number, label = index + 1) {
  await page.locator(".view .wrap > svg g.edge line.hit").nth(index).dispatchEvent("pointerdown", { button: 0 });
  await expect(bar(page, "def.title").locator(".which strong")).toHaveText(cs("def.wall", { n: label }));
}

/** Opens a job by id in the editor (library row double-click). */
export async function openEditor(app: App, name: string) {
  await app.open();
  await app.openJob(name);
  await expect(plan(app.page)).toBeVisible();
}

/** The total roof area shown in the results tab, in m². */
export async function shownArea(page: Page): Promise<number> {
  const text = (await page.locator(".results .hero strong").textContent()) ?? "";
  return parseCoord(text.replace("m²", ""));
}

/**
 * The roof area once the first calculation has been shown. Right after the
 * editor opens the results may still show a placeholder, which reads as NaN.
 */
export async function firstArea(page: Page): Promise<number> {
  await expect.poll(() => shownArea(page)).toBeGreaterThan(0);
  return shownArea(page);
}
