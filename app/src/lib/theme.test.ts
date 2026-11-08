// Theme tokens: the dark theme picked in Settings is the same palette as the
// dark theme that follows the OS, and components take colours only from the
// tokens in app.css (AGENTS.md, Rules).
import { readFileSync, readdirSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const LIB = path.dirname(fileURLToPath(import.meta.url));
const CSS = readFileSync(path.join(LIB, "..", "app.css"), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");

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
    expect(picked).toEqual(system);
  });
});

describe("components", () => {
  it("use no raw colours in their styles (tokens from app.css only)", () => {
    const found: string[] = [];
    for (const f of readdirSync(LIB).filter((f) => f.endsWith(".svelte") && f !== "Logo.svelte")) {
      const src = readFileSync(path.join(LIB, f), "utf8");
      for (const style of src.matchAll(/<style[^>]*>([\s\S]*?)<\/style>/g)) {
        for (const m of style[1].matchAll(/#[0-9a-fA-F]{3,8}\b/g)) found.push(`${f}: ${m[0]}`);
      }
    }
    expect(found).toEqual([]);
  });
});
