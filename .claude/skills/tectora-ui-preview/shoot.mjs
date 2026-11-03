#!/usr/bin/env node
// Screenshot matrix of the browser preview (npm run preview must be running).
//
//   node .claude/skills/tectora-ui-preview/shoot.mjs [options]
//     --screens home,editor          home = job library, editor = first job opened
//     --sizes 1360x860,960x540       window sizes (min supported window 600x420)
//     --locales cs,sk,en,de
//     --themes light,dark
//     --click "<selector>"           extra clicks after the screen opened (repeatable)
//     --out target/ui-shots          output folder
//     --url http://localhost:1430/
//
// Prints one line per shot with layout problems found: page scroll,
// elements outside the window, truncated buttons, unexpected horizontal
// scrollbars, page errors and console errors.
import { createRequire } from "node:module";
import { execSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

function loadPlaywright() {
  const require = createRequire(import.meta.url);
  try {
    return require("playwright");
  } catch {
    const root = execSync("npm root -g").toString().trim();
    return require(path.join(root, "playwright"));
  }
}

function options(argv) {
  const o = {
    screens: "home,editor",
    sizes: "1360x860",
    locales: "cs",
    themes: "light",
    out: "target/ui-shots",
    url: "http://localhost:1430/",
    click: [],
  };
  for (let i = 0; i < argv.length; i += 2) {
    const key = argv[i].replace(/^--/, "");
    if (!(key in o)) throw new Error(`unknown option ${argv[i]}`);
    if (key === "click") o.click.push(argv[i + 1]);
    else o[key] = argv[i + 1];
  }
  return o;
}

async function diagnose(page) {
  return page.evaluate(() => {
    const W = innerWidth;
    const H = innerHeight;
    const out = [];
    const doc = document.documentElement;
    if (doc.scrollWidth > W) out.push(`page scrollWidth ${doc.scrollWidth} > ${W}`);
    if (doc.scrollHeight > H) out.push(`page scrollHeight ${doc.scrollHeight} > ${H}`);
    const scrolls = (el) => {
      for (let p = el.parentElement; p; p = p.parentElement) {
        const s = getComputedStyle(p);
        if (/(auto|scroll)/.test(s.overflowX + s.overflowY) && (p.scrollWidth > p.clientWidth || p.scrollHeight > p.clientHeight)) return true;
      }
      return false;
    };
    for (const el of document.querySelectorAll("button, input, select, label, h1, h2, h3")) {
      const r = el.getBoundingClientRect();
      if (!r.width || getComputedStyle(el).visibility === "hidden" || scrolls(el)) continue;
      const text = (el.textContent || "").trim().slice(0, 30);
      if (r.right > W + 1 || r.bottom > H + 1) out.push(`outside window <${el.tagName.toLowerCase()}> "${text}"`);
      if (el.tagName === "BUTTON" && el.scrollWidth > el.clientWidth + 1 && getComputedStyle(el).overflow !== "visible") out.push(`truncated button "${text}"`);
    }
    for (const el of document.querySelectorAll("*")) {
      const s = getComputedStyle(el);
      if (/(auto|scroll)/.test(s.overflowX) && el.scrollWidth > el.clientWidth + 2) out.push(`h-scroll <${el.tagName.toLowerCase()} class="${el.className}">`);
    }
    return out.slice(0, 25);
  });
}

const o = options(process.argv.slice(2));
const { chromium } = loadPlaywright();
fs.mkdirSync(o.out, { recursive: true });
const browser = await chromium.launch();
let problems = 0;
for (const screen of o.screens.split(","))
  for (const size of o.sizes.split(","))
    for (const locale of o.locales.split(","))
      for (const theme of o.themes.split(",")) {
        const [width, height] = size.split("x").map(Number);
        const ctx = await browser.newContext({ viewport: { width, height }, colorScheme: theme });
        const page = await ctx.newPage();
        const errors = [];
        page.on("pageerror", (e) => errors.push(`page error: ${e.message}`));
        page.on("console", (m) => m.type() === "error" && errors.push(`console: ${m.text()}`));
        await page.addInitScript((l) => localStorage.setItem("tectora.locale", l), locale);
        await page.goto(o.url);
        await page.waitForTimeout(1200);
        if (screen === "editor") {
          await page.locator("tbody tr").first().dblclick({ timeout: 5000 }).catch(() => errors.push("no job row to open"));
          await page.waitForTimeout(1500);
        }
        for (const sel of o.click) {
          await page.locator(sel).first().click({ timeout: 3000 }).catch(() => errors.push(`click failed: ${sel}`));
          await page.waitForTimeout(500);
        }
        const found = [...errors, ...(await diagnose(page))];
        problems += found.length;
        const name = `${screen}_${size}_${locale}_${theme}.png`;
        await page.screenshot({ path: path.join(o.out, name) });
        console.log(found.length ? `${name}\n   ${found.join("\n   ")}` : `${name}: ok`);
        await ctx.close();
      }
await browser.close();
process.exit(problems ? 1 : 0);
