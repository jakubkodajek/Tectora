// Theme tokens: the dark theme picked in Settings is the same palette as the
// dark theme that follows the OS, and components take colours only from the
// tokens in app.css (AGENTS.md, Rules).
import { describe, expect, it } from "vitest";

// Vitest stubs CSS imports (even `?raw`), so read the file itself. The
// module names are built at run time: the UI has no Node type definitions.
const fs: { readFileSync(p: URL, enc: "utf8"): string } = await import(/* @vite-ignore */ ["node", "fs"].join(":"));
const appCss = fs.readFileSync(new URL("../app.css", import.meta.url), "utf8");
const CSS = appCss.replace(/\/\*[\s\S]*?\*\//g, "");
const components = import.meta.glob(["./*.svelte", "!./Logo.svelte"], { query: "?raw", import: "default", eager: true }) as Record<string, string>;

/** Custom properties declared in the first block whose selector matches `selector`. */
function tokens(selector: RegExp): Record<string, string> {
  const m = selector.exec(CSS);
  if (!m) throw new Error(`no block ${selector}`);
  const open = CSS.indexOf("{", m.index + m[0].length - 1);
  const body = CSS.slice(open + 1, CSS.indexOf("}", open));
  return Object.fromEntries([...body.matchAll(/--([\w-]+)\s*:\s*([^;]+);/g)].map((t) => [t[1], t[2].trim()]));
}

describe("theme tokens", () => {
  const system = tokens(/@media \(prefers-color-scheme: dark\)\s*\{\s*:root:not\(\[data-theme="light"\]\)\s*\{/);
  const picked = tokens(/^:root\[data-theme="dark"\]\s*\{/m);

  it("the dark theme picked in Settings equals the system dark theme", () => {
    expect(Object.keys(system).length).toBeGreaterThan(20);
    expect(picked).toEqual(system);
  });
});

describe("components", () => {
  it("use no raw colours in their styles (tokens from app.css only)", () => {
    const found: string[] = [];
    for (const [f, src] of Object.entries(components)) {
      for (const style of src.matchAll(/<style[^>]*>([\s\S]*?)<\/style>/g)) {
        for (const m of style[1].matchAll(/#[0-9a-fA-F]{3,8}\b/g)) found.push(`${f}: ${m[0]}`);
      }
    }
    expect(found).toEqual([]);
  });
});
