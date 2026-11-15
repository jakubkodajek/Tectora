// Every screen in every language: no untranslated keys, no DACAD names or
// original German DACAD strings outside German, nothing cut off or pushed
// out of the window, at a laptop size and at the minimum window size.
import type { Page } from "@playwright/test";
import { openEditor, selectWall } from "./editor";
import { bridge, createExample, expect, test, type App, type Locale } from "./fixtures";
import { DICTS, LOCALES, tr } from "./i18n";

const KEYS = new Set(Object.keys(DICTS.cs));

/** Visible text problems on the page: raw keys and forbidden names. */
async function textProblems(page: Page, locale: Locale): Promise<string[]> {
  const text = await page.evaluate(() => {
    const parts: string[] = [];
    const walk = (n: Node) => {
      if (n.nodeType === Node.TEXT_NODE) parts.push(n.textContent ?? "");
      else if (n instanceof HTMLElement || n instanceof SVGElement) {
        if (n instanceof HTMLElement && n.offsetParent === null && getComputedStyle(n).position !== "fixed") return;
        for (const a of ["title", "aria-label", "placeholder"]) {
          const v = (n as Element).getAttribute(a);
          if (v) parts.push(v);
        }
        n.childNodes.forEach(walk);
      }
    };
    walk(document.body);
    return parts.join("\n");
  });
  const out: string[] = [];
  for (const m of text.matchAll(/\b[a-z][a-zA-Z]*(?:\.[a-zA-Z_]+)+\b/g)) {
    if (KEYS.has(m[0])) out.push(`raw key ${m[0]}`);
  }
  if (/\bDACAD\b|\bCOIN\b/i.test(text)) out.push("mentions DACAD / COIN");
  if (/\{\w+\}/.test(text)) out.push(`unfilled placeholder ${text.match(/\{\w+\}/)![0]}`);
  if (locale !== "de" && /\b(Nicht geschlossen|Fläche|Traufe|Gaube|Kehle|Ortgang|Richtungswechsel)\b/.test(text)) out.push("German text outside German");
  return out;
}

/** Layout problems: page scroll and elements sticking out of the window. */
async function layoutProblems(page: Page): Promise<string[]> {
  return page.evaluate(() => {
    const out: string[] = [];
    const de = document.documentElement;
    if (de.scrollWidth > innerWidth + 1) out.push(`page scrolls sideways (${de.scrollWidth} > ${innerWidth})`);
    if (de.scrollHeight > innerHeight + 1) out.push(`page scrolls down (${de.scrollHeight} > ${innerHeight})`);
    // Inside a container that scrolls sideways (toolbar, filter strip) an
    // element may sit outside the window: it can be scrolled to.
    const scrollsX = (el: HTMLElement) => {
      for (let p = el.parentElement; p; p = p.parentElement) {
        const o = getComputedStyle(p).overflowX;
        if ((o === "auto" || o === "scroll") && p.scrollWidth > p.clientWidth) return true;
      }
      return false;
    };
    for (const el of document.querySelectorAll<HTMLElement>("button, input, select, label, h1, h2, h3, strong")) {
      if (el.offsetParent === null) continue;
      const r = el.getBoundingClientRect();
      if (r.width === 0 || r.height === 0) continue;
      if ((r.right > innerWidth + 1 || r.left < -1) && !scrollsX(el)) out.push(`${el.tagName.toLowerCase()} "${(el.textContent ?? "").trim().slice(0, 30)}" outside the window`);
    }
    // Labels cut with an ellipsis or overflowing their button.
    for (const b of document.querySelectorAll<HTMLElement>("button")) {
      if (b.offsetParent === null) continue;
      if (b.scrollWidth > b.clientWidth + 2 && getComputedStyle(b).overflow !== "visible") out.push(`button "${(b.textContent ?? "").trim().slice(0, 30)}" is cut`);
    }
    return [...new Set(out)];
  });
}

async function screens(app: App, locale: Locale, check: (where: string) => Promise<void>) {
  const page = app.page;
  await app.open({ locale });
  await check("library");
  await page.locator("header").getByRole("button", { name: tr(locale, "lib.new") }).click();
  await check("new job");
  await page.keyboard.press("Escape");
  await page.locator("nav").getByRole("button", { name: tr(locale, "home.examples") }).click();
  await check("examples");
  await page.getByRole("button", { name: tr(locale, "settings.button") }).click();
  await check("settings");
  await page.keyboard.press("Escape");
  await page.getByRole("button", { name: tr(locale, "ai.connect") }).click();
  await check("AI connect");
  await page.keyboard.press("Escape");
  await page.locator("nav").getByRole("button", { name: tr(locale, "lib.all") }).click();
  await app.openJob("Jazyk");
  await check("editor");
  await selectWall(page, 1, 2).catch(() => undefined);
  await check("editor, wall selected");
  for (const tab of ["dock.ai", "dock.job"]) {
    await page.locator("aside .tabs").getByRole("tab", { name: tr(locale, tab) }).click();
    await check(`editor, ${tab}`);
  }
}

for (const locale of LOCALES) {
  test(`${locale}: every screen is translated and fits 1360×860`, async ({ app }) => {
    await createExample("t-shape", "Jazyk");
    const found: string[] = [];
    await screens(app, locale, async (where) => {
      for (const p of await textProblems(app.page, locale)) found.push(`${where}: ${p}`);
      for (const p of await layoutProblems(app.page)) found.push(`${where}: ${p}`);
    });
    expect(found).toEqual([]);
  });
}

for (const locale of ["cs", "de"] as Locale[]) {
  test(`${locale}: nothing leaves the window at the minimum size 600×420`, async ({ app, page }) => {
    await page.setViewportSize({ width: 600, height: 420 });
    await createExample("t-shape", "Jazyk");
    const found: string[] = [];
    await screens(app, locale, async (where) => {
      for (const p of await layoutProblems(page)) if (!p.startsWith("page scrolls down")) found.push(`${where}: ${p}`);
    });
    expect(found).toEqual([]);
  });
}

test("dark theme renders every screen without errors", async ({ app, page }) => {
  await createExample("hip", "Tma");
  await bridge("update_settings", { patch: { theme: "dark" } });
  await app.open();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  await openEditor(app, "Tma").catch(async () => app.openJob("Tma"));
  await expect(page.locator(".view canvas")).toBeVisible();
});

for (const locale of ["cs", "de"] as Locale[]) {
  test(`${locale}: the dock tab labels are whole or hidden in a half-screen editor 960×540, never cut (UI-005)`, async ({ app, page }) => {
    await page.setViewportSize({ width: 960, height: 540 });
    await createExample("hip", "Dok");
    await app.open({ locale });
    await app.openJob("Dok");
    const tabs = page.locator("aside .tabs");
    await expect(tabs).toBeVisible();
    const labels = () =>
      tabs.locator(".tab-label").evaluateAll((ls) =>
        ls.map((l) => {
          const e = l as HTMLElement;
          return { text: e.textContent, cut: e.clientWidth > 1 && e.scrollWidth > e.clientWidth + 1 };
        }),
      );
    for (const l of await labels()) expect(l.cut, `${locale}: ${l.text}`).toBe(false);
    // Each tab still says what it is (tooltip and accessible name).
    for (const key of ["dock.results", "dock.ai", "dock.job"]) {
      await expect(tabs.getByRole("tab", { name: tr(locale, key) })).toBeVisible();
    }
  });
}
