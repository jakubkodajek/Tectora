// Guards for the round-9 audit (2026-11-22), input side.
//
// HUNT-802 / HUNT-804: a job whose numbers JavaScript changes on its round
// trip (a whole float like `1.0` in an unknown key, a stored result dated
// after 2^53 ms) saves from the editor; the impossible date is dropped with
// one warning when the job opens.
// UX-805: + / − on the keyboard zoom the plan, as its hint says; after a
// finger or pen press the hint names long press first.
// TEST-805: a pen held still on a wall opens its menu; a mouse held still
// opens none.
import fs from "node:fs";
import path from "node:path";
import type { Page } from "@playwright/test";
import { planXY } from "./editor";
import { LIBRARY_DIR } from "./env";
import { createExample, expect, loadProject, test, waitForProject } from "./fixtures";
import { cs } from "./i18n";

const jobTab = (page: Page) => page.locator("aside .tabs").getByRole("tab", { name: cs("dock.job") });

test.describe("round 9: numbers JavaScript changes (HUNT-802, HUNT-804)", () => {
  test("a job with 1.0 in an unknown key and a result dated after 2^53 ms saves from the editor", async ({ app, page }) => {
    const id = await createExample("gable", "Čísla");
    const file = path.join(LIBRARY_DIR, `${id}.tectora`);
    const p = JSON.parse(fs.readFileSync(file, "utf8"));
    p.result_snapshot = { engine_version: "0.9.0", taken_ms: 4242424242, roof_area: 99, ridge_height: 5, lines_by_code: { T: 20 } };
    // Written as another tool would: JSON.stringify can write neither.
    const text = JSON.stringify(p, null, 2)
      .replace("4242424242", "9007199254740993")
      .replace("{", '{\n  "vendor_scale": 1.0,\n  "vendor_o": {"a": [2.0, -0.0, 1e3]},');
    fs.writeFileSync(file, text);

    await app.open({ prefs: { followAi: false } });
    await app.openJob("Čísla");
    const warning = app.toasts().filter({ hasText: cs("err.result_dropped") });
    await expect(warning).toHaveCount(1);
    await expect(warning).toContainText("result_dropped");

    await jobTab(page).click();
    const customer = page.locator("aside .job").getByLabel(cs("job.customer"));
    await customer.click();
    await page.keyboard.type("Novák");
    await page.keyboard.press("Tab");
    const saved = await waitForProject(id, (q) => q.job?.customer === "Novák");
    await app.saved();
    expect(saved.result_snapshot ?? null).toBeNull();
    expect(saved.vendor_scale).toBe(1);
    expect(saved.vendor_o).toEqual({ a: [2, 0, 1000] });
    // No conflict was told, and the next open has nothing to warn about.
    await expect(app.toasts().filter({ hasText: "save_conflict" })).toHaveCount(0);
    expect((await loadProject(id)).job.customer).toBe("Novák");
  });
});

/** Plan pixels per metre, from two world points. */
async function pxPerMetre(page: Page): Promise<number> {
  const a = await planXY(page, 0, 0);
  const b = await planXY(page, 10, 0);
  return Math.hypot(b.x - a.x, b.y - a.y) / 10;
}

test.describe("round 9: plan zoom keys and hint (UX-805)", () => {
  test("+ and − on the keyboard zoom the plan as the toolbar buttons do; in a field they are typed", async ({ app, page }) => {
    await createExample("gable", "Zoom");
    await app.open({ prefs: { followAi: false } });
    await app.openJob("Zoom");
    await app.saved();
    const k0 = await pxPerMetre(page);
    await page.keyboard.press("+");
    await expect.poll(() => pxPerMetre(page)).toBeGreaterThan(k0 * 1.2);
    const k1 = await pxPerMetre(page);
    await page.keyboard.press("-");
    await expect.poll(async () => Math.abs((await pxPerMetre(page)) / k0 - 1)).toBeLessThan(0.02);
    // A key typed in a field is the field's.
    await jobTab(page).click();
    const customer = page.locator("aside .job").getByLabel(cs("job.customer"));
    await customer.click();
    await page.keyboard.type("A-B+");
    await expect(customer).toHaveValue("A-B+");
    await expect.poll(async () => Math.abs((await pxPerMetre(page)) / k0 - 1)).toBeLessThan(0.02);
    expect(k1).toBeGreaterThan(k0);
  });
});

test.describe("round 9: a pen and a mouse held still (TEST-805)", () => {
  test("a pen held still on a wall opens its menu and the hint names long press first", async ({ app, page }) => {
    await createExample("gable", "Pero");
    await app.open({ prefs: { followAi: false } });
    await app.openJob("Pero");
    const hint = page.getByTestId("plan-hint");
    await expect(hint).toHaveText(cs("plan.hint"));
    const w = await planXY(page, 6, 0);
    const cdp = await page.context().newCDPSession(page);
    const ev = (type: string, x: number, y: number, extra: Record<string, unknown> = {}) =>
      cdp.send("Input.dispatchMouseEvent", { type, x, y, pointerType: "pen", button: "left", buttons: type === "mouseReleased" ? 0 : 1, clickCount: 1, ...extra } as any);
    await ev("mouseMoved", w.x, w.y, { button: "none", buttons: 0 });
    await ev("mousePressed", w.x, w.y);
    await page.waitForTimeout(900);
    const menu = page.getByRole("menu");
    await expect(menu).toHaveCount(1);
    await expect(menu.getByRole("menuitem", { name: cs("menu.insertPoint") })).toBeVisible();
    await ev("mouseReleased", w.x, w.y);
    await page.keyboard.press("Escape");
    await expect(menu).toHaveCount(0);
    await expect(hint).toHaveText(cs("plan.hintTouch"));
  });

  test("a mouse held still on a wall opens no menu", async ({ app, page }) => {
    await createExample("gable", "Myš");
    await app.open({ prefs: { followAi: false } });
    await app.openJob("Myš");
    const w = await planXY(page, 6, 0);
    await page.mouse.move(w.x, w.y);
    await page.mouse.down();
    await page.waitForTimeout(900);
    await expect(page.getByRole("menu")).toHaveCount(0);
    await page.mouse.up();
    await expect(page.getByRole("menu")).toHaveCount(0);
    await expect(page.getByTestId("plan-hint")).toHaveText(cs("plan.hint"));
  });
});
