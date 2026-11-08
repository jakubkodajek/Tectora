// App shell: the editor toolbar and the library filter strip at laptop and
// half-screen sizes, the library list from the keyboard, dialogs and menus
// (focus, Escape), toggle and tab state for assistive tech, the settings
// roof defaults and the empty-filter message.
import type { Locator, Page } from "@playwright/test";
import { createExample, expect, test, type App } from "./fixtures";
import { cs, tr } from "./i18n";

// Headless Chromium hides all scrollbars; these tests check that strips
// which scroll sideways show theirs, as the desktop web views do.
test.use({ launchOptions: { ignoreDefaultArgs: ["--hide-scrollbars"] } });

const startsWith = (s: string) => new RegExp(`^\\s*${s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}`);

/** Tool buttons that lie outside the toolbar's visible box (scrolled away or cut). */
async function hiddenTools(page: Page): Promise<string[]> {
  return page.evaluate(() => {
    const bar = document.querySelector<HTMLElement>(".editor .toolbar")!;
    const b = bar.getBoundingClientRect();
    return [...bar.querySelectorAll<HTMLElement>(".tool")]
      .filter((el) => {
        const r = el.getBoundingClientRect();
        return r.right > Math.min(b.right, innerWidth) + 1 || r.left < b.left - 1;
      })
      .map((el) => el.getAttribute("title") ?? "");
  });
}

/** A horizontally scrolling strip shows a scrollbar (not an overlay that hides). */
async function showsScrollbar(el: Locator): Promise<boolean> {
  return el.evaluate((e) => {
    const s = getComputedStyle(e);
    const borders = parseFloat(s.borderTopWidth) + parseFloat(s.borderBottomWidth);
    return e.offsetHeight - e.clientHeight - borders > 0;
  });
}

async function openJobByName(app: App, name: string) {
  await app.openJob(name);
  await expect(app.page.locator(".view .wrap > svg").first()).toBeVisible();
}

test.describe("shell layout", () => {
  for (const [w, h] of [
    [1360, 860],
    [1440, 900],
  ]) {
    test(`de: every toolbar button is visible at ${w}×${h}`, async ({ app, page }) => {
      await page.setViewportSize({ width: w, height: h });
      await createExample("hip", "Werkzeug");
      await app.open({ locale: "de" });
      await openJobByName(app, "Werkzeug");
      expect(await hiddenTools(page)).toEqual([]);
    });
  }

  test("cs at 1440×900 keeps the labelled toolbar", async ({ app, page }) => {
    await page.setViewportSize({ width: 1440, height: 900 });
    await createExample("hip", "Popisky");
    await app.open();
    await openJobByName(app, "Popisky");
    await expect(page.locator(".editor .toolbar .tool span").first()).toBeVisible();
    const w = await page.locator(".editor .toolbar .tool span").first().evaluate((e) => e.getBoundingClientRect().width);
    expect(w).toBeGreaterThan(5);
    expect(await hiddenTools(page)).toEqual([]);
  });

  test("a toolbar that does not fit scrolls visibly and with the mouse wheel", async ({ app, page }) => {
    await page.setViewportSize({ width: 600, height: 420 });
    await createExample("hip", "Malé okno");
    await app.open();
    await openJobByName(app, "Malé okno");
    const bar = page.locator(".editor .toolbar");
    const overflows = await bar.evaluate((e) => e.scrollWidth > e.clientWidth + 1);
    expect(overflows).toBe(true);
    expect(await showsScrollbar(bar)).toBe(true);
    await bar.hover();
    await page.mouse.wheel(0, 400);
    await expect.poll(() => bar.evaluate((e) => e.scrollLeft)).toBeGreaterThan(0);
  });

  test("de at 960×540: every library filter can be reached", async ({ app, page }) => {
    await page.setViewportSize({ width: 960, height: 540 });
    await createExample("hip", "Filtr");
    await app.open({ locale: "de" });
    const nav = page.locator(".layout > nav");
    const examples = nav.getByRole("button", { name: tr("de", "home.examples") });
    const inWindow = () => examples.evaluate((e) => e.getBoundingClientRect().right <= innerWidth + 1);
    if (!(await inWindow())) {
      expect(await showsScrollbar(nav)).toBe(true);
      await nav.hover();
      await page.mouse.wheel(0, 600);
      await expect.poll(inWindow).toBe(true);
    }
    await examples.click();
    await expect(page.locator("main h2")).toHaveText(tr("de", "home.examples"));
  });
});

test.describe("library list", () => {
  test("double-click opens a lower row at 600×420", async ({ app, page }) => {
    await page.setViewportSize({ width: 600, height: 420 });
    for (const n of ["Jedna", "Dvě", "Tři", "Čtyři", "Pět"]) await createExample("gable", n);
    await app.open();
    const main = page.locator(".layout > main");
    const box = (await main.boundingBox())!;
    // The lowest row that is fully visible before anything is selected.
    const rows = page.locator("tbody tr");
    let target: Locator | null = null;
    for (let i = (await rows.count()) - 1; i >= 0; i--) {
      const r = (await rows.nth(i).boundingBox())!;
      if (r.y + r.height <= box.y + box.height) {
        target = rows.nth(i);
        break;
      }
    }
    expect(target).not.toBeNull();
    await target!.dblclick();
    await expect(page.locator(".editor .statusbar")).toBeVisible();
  });

  test("selecting a row does not move the rows under the pointer at 600×420", async ({ app, page }) => {
    await page.setViewportSize({ width: 600, height: 420 });
    for (const n of ["A1", "A2", "A3", "A4", "A5"]) await createExample("gable", n);
    await app.open();
    const rows = page.locator("tbody tr");
    const n = await rows.count();
    const main = (await page.locator(".layout > main").boundingBox())!;
    for (let i = 0; i < n; i++) {
      const r = (await rows.nth(i).boundingBox())!;
      if (r.y + r.height > main.y + main.height) break;
      const x = r.x + r.width / 2;
      const y = r.y + r.height / 2;
      await page.mouse.click(x, y);
      await expect(rows.nth(i)).toHaveClass(/sel/);
      // The second press of a double-click lands on the same row, never on
      // an action of the detail strip (Duplicate, Delete).
      const under = await page.evaluate(([px, py]) => document.elementFromPoint(px, py)?.closest("tr")?.rowIndex ?? -1, [x, y]);
      expect(under, `row ${i + 1}`).toBe(i + 1);
    }
  });

  test("rows are reached with Tab, moved with arrows and opened with Enter", async ({ app, page }) => {
    await createExample("gable", "Klávesa A");
    await createExample("hip", "Klávesa B");
    await app.open();
    const search = page.getByPlaceholder(cs("lib.search"));
    await search.focus();
    let onRow = false;
    for (let i = 0; i < 40 && !onRow; i++) {
      await page.keyboard.press("Tab");
      onRow = await page.evaluate(() => document.activeElement?.closest("tbody tr") !== null);
    }
    expect(onRow).toBe(true);
    const first = await page.evaluate(() => document.activeElement!.textContent);
    await page.keyboard.press("ArrowDown");
    const second = await page.evaluate(() => document.activeElement!.textContent);
    expect(second).not.toBe(first);
    await expect(page.locator("tbody tr.sel")).toHaveCount(1);
    await page.keyboard.press("Enter");
    await expect(page.locator(".editor .statusbar")).toBeVisible();
  });

  test("an empty status filter names the status, not an empty search", async ({ app, page }) => {
    await createExample("gable", "Koncept");
    await app.open();
    await page.locator("nav").getByRole("button", { name: startsWith(cs("status.ordered")) }).click();
    const empty = page.locator("main .empty");
    await expect(empty).toHaveText(cs("home.noneInFilter", { filter: cs("status.ordered") }));
    await page.locator("nav").getByRole("button", { name: startsWith(cs("lib.byAi")) }).click();
    await expect(empty).toHaveText(cs("home.noneInFilter", { filter: cs("lib.byAi") }));
    await page.getByPlaceholder(cs("lib.search")).fill("xyz");
    await expect(empty).toHaveText(cs("home.noMatch", { q: "xyz" }));
  });
});

test.describe("dialogs and menus", () => {
  test("a dialog takes the focus, keeps Tab inside and gives the focus back", async ({ app, page }) => {
    await app.open();
    const opener = page.getByRole("button", { name: cs("settings.button") });
    await opener.click();
    const dlg = page.getByRole("dialog", { name: cs("settings.title") });
    await expect(dlg).toBeVisible();
    const inside = () => page.evaluate(() => !!document.activeElement?.closest("[role=dialog]"));
    await expect.poll(inside).toBe(true);
    for (let i = 0; i < 40; i++) {
      await page.keyboard.press("Tab");
      expect(await inside(), `Tab ${i + 1}`).toBe(true);
    }
    for (let i = 0; i < 10; i++) {
      await page.keyboard.press("Shift+Tab");
      expect(await inside(), `Shift+Tab ${i + 1}`).toBe(true);
    }
    await page.keyboard.press("Escape");
    await expect(dlg).toBeHidden();
    await expect(opener).toBeFocused();
  });

  test("new job dialog starts in its first field", async ({ app, page }) => {
    await app.open();
    await page.locator("header").getByRole("button", { name: cs("lib.new") }).click();
    const dlg = page.getByRole("dialog", { name: cs("new.title") });
    await expect.poll(() => page.evaluate(() => document.activeElement?.closest("[role=dialog]") !== null && document.activeElement?.tagName)).toBe("INPUT");
    await dlg.getByRole("button", { name: cs("common.close") }).click();
    await expect(page.locator("header").getByRole("button", { name: cs("lib.new") })).toBeFocused();
  });

  test("the card menu opens on its first item, moves with arrows and closes with Escape", async ({ app, page }) => {
    await createExample("gable", "Karta");
    await app.open({ prefs: { libraryView: "grid" } });
    const more = page.getByRole("button", { name: cs("card.more") });
    await more.click();
    const menu = page.getByRole("menu");
    await expect(menu).toBeVisible();
    await expect(menu.getByRole("menuitem").first()).toBeFocused();
    await page.keyboard.press("ArrowDown");
    await expect(menu.getByRole("menuitem").nth(1)).toBeFocused();
    await page.keyboard.press("ArrowUp");
    await page.keyboard.press("ArrowUp");
    await expect(menu.getByRole("menuitem").last()).toBeFocused();
    await page.keyboard.press("Escape");
    await expect(menu).toBeHidden();
    await expect(more).toBeFocused();
  });
});

test.describe("state for assistive tech", () => {
  test("toolbar toggles say whether they are on; dock tabs follow the tabs pattern", async ({ app, page }) => {
    await createExample("hip", "Přepínače");
    await app.open();
    await openJobByName(app, "Přepínače");
    const tb = page.getByRole("toolbar").first();
    const dims = tb.getByRole("button", { name: cs("tool.dims"), exact: true });
    const before = await dims.getAttribute("aria-pressed");
    expect(["true", "false"]).toContain(before);
    await dims.click();
    await expect(dims).toHaveAttribute("aria-pressed", before === "true" ? "false" : "true");
    for (const key of ["editor.plan", "editor.both", "tool.flat", "tool.standard", "tool.draw", "tool.dormer"]) {
      const b = tb.getByRole("button", { name: cs(key), exact: true });
      expect(["true", "false"], key).toContain(await b.getAttribute("aria-pressed"));
    }
    // Plain commands are not toggles.
    expect(await tb.getByRole("button", { name: cs("editor.undo"), exact: true }).getAttribute("aria-pressed")).toBeNull();

    const tabs = page.locator("aside .tabs");
    const results = tabs.getByRole("tab", { name: cs("dock.results") });
    await results.click();
    await expect(results).toHaveAttribute("aria-selected", "true");
    const panel = page.getByRole("tabpanel");
    await expect(panel).toHaveCount(1);
    await expect(panel).toHaveAttribute("aria-labelledby", (await results.getAttribute("id"))!);
    await expect(results).toHaveAttribute("aria-controls", (await panel.getAttribute("id"))!);
    await results.focus();
    await page.keyboard.press("ArrowRight");
    const next = tabs.getByRole("tab", { name: cs("dock.breakdown") });
    await expect(next).toBeFocused();
    await expect(next).toHaveAttribute("aria-selected", "true");
    await expect(results).toHaveAttribute("tabindex", "-1");
    await page.keyboard.press("End");
    await expect(tabs.getByRole("tab", { name: cs("dock.job") })).toHaveAttribute("aria-selected", "true");
    await page.keyboard.press("Home");
    await expect(results).toHaveAttribute("aria-selected", "true");
  });

  test("new job tabs move with the arrow keys", async ({ app, page }) => {
    await app.open();
    await page.locator("header").getByRole("button", { name: cs("lib.new") }).click();
    const dlg = page.getByRole("dialog", { name: cs("new.title") });
    const first = dlg.getByRole("tab", { name: cs("new.standard") });
    await first.focus();
    await page.keyboard.press("ArrowRight");
    await expect(dlg.getByRole("tab", { name: cs("new.example") })).toHaveAttribute("aria-selected", "true");
    await expect(dlg.getByRole("tabpanel")).toHaveCount(1);
  });
});

test.describe("settings roof defaults", () => {
  test("a refused default names its allowed range on the field", async ({ app, page }) => {
    await app.open();
    await page.getByRole("button", { name: cs("settings.button") }).click();
    const dlg = page.getByRole("dialog", { name: cs("settings.title") });
    const pitch = dlg.locator(".grid3").getByLabel(cs("edge.pitch"));
    await pitch.fill("95");
    await pitch.press("Tab");
    await expect(pitch).toHaveAttribute("aria-invalid", "true");
    const range = cs("num.range", { min: "1", max: "89" });
    await expect(pitch).toHaveAttribute("title", range);
    await expect(dlg.getByText(range)).toBeVisible();
    await expect(dlg.getByRole("alert")).toHaveText(cs("settings.err.invalid_value"));
    const over = dlg.locator(".grid3").getByLabel(cs("edge.overhang"));
    await over.fill("25");
    await over.press("Tab");
    await expect(dlg.getByText(cs("num.range", { min: "0", max: "20" }))).toBeVisible();
  });
});
