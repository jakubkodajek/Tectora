/// <reference types="vite/client" />
import { describe, expect, it } from "vitest";
import { formatNumber, parseNumber, stepValue } from "./numeric.svelte";

describe("parseNumber", () => {
  it("takes a comma or a dot as the decimal separator", () => {
    expect(parseNumber("2,5")).toBe(2.5);
    expect(parseNumber("2.5")).toBe(2.5);
    expect(parseNumber(" 0,05 ")).toBe(0.05);
    expect(parseNumber(",5")).toBe(0.5);
    expect(parseNumber("5,")).toBe(5);
  });

  it("ignores spaces and reads the minus sign", () => {
    expect(parseNumber("1 250,5")).toBe(1250.5);
    expect(parseNumber("1 250")).toBe(1250);
    expect(parseNumber("-1,5")).toBe(-1.5);
    expect(parseNumber("−1,5")).toBe(-1.5);
  });

  it("refuses text that is not one number", () => {
    for (const s of ["", " ", "abc", "1,2,3", "1.2.3", "2m", "--1", "Infinity", "NaN", "1e999"]) {
      expect(parseNumber(s), s).toBeNull();
    }
  });
});

describe("formatNumber", () => {
  it("writes the UI language's decimal separator, no grouping", () => {
    expect(formatNumber(2.5, "cs-CZ")).toBe("2,5");
    expect(formatNumber(2.5, "en-GB")).toBe("2.5");
    expect(formatNumber(1250.5, "de-DE")).toBe("1250,5");
    expect(formatNumber(12, "cs-CZ", 2)).toBe("12,00");
  });

  it("rounds to 3 decimals and leaves the field empty for no value", () => {
    expect(formatNumber(1 / 3, "en-GB")).toBe("0.333");
    expect(formatNumber(null, "cs-CZ")).toBe("");
    expect(formatNumber(undefined, "cs-CZ")).toBe("");
    expect(formatNumber(NaN, "cs-CZ")).toBe("");
  });

  it("reads back what it writes", () => {
    for (const loc of ["cs-CZ", "sk-SK", "en-GB", "de-DE"]) {
      for (const v of [0, 0.05, 2.5, -3.125, 1234.5]) expect(parseNumber(formatNumber(v, loc)), `${loc} ${v}`).toBe(v);
    }
  });
});

describe("stepValue", () => {
  it("steps without floating-point noise", () => {
    expect(stepValue(2.3, 1, 0.05)).toBe(2.35);
    expect(stepValue(0.1, 1, 0.2)).toBe(0.3);
    expect(stepValue(40, -1, 1)).toBe(39);
  });

  it("keeps the precision of a typed value", () => {
    expect(stepValue(2.123, 1, 0.05)).toBe(2.173);
  });

  it("clamps to the bounds", () => {
    expect(stepValue(0, -1, 0.05, 0)).toBe(0);
    expect(stepValue(89, 1, 1, 1, 89)).toBe(89);
  });
});

// Every number field must use the shared action: a native number input loses
// typed values in WebKit (macOS) and refuses the decimal comma in Chromium.
const sources = import.meta.glob(["../**/*.svelte"], { query: "?raw", import: "default", eager: true }) as Record<string, string>;

describe("number fields", () => {
  it("reads the UI sources", () => {
    expect(Object.keys(sources).some((f) => f.endsWith("RoofDefBar.svelte"))).toBe(true);
  });

  it("no native number input: all go through use:numeric", () => {
    const bad: string[] = [];
    for (const [file, text] of Object.entries(sources)) {
      text.split("\n").forEach((line, i) => {
        if (/type=["']number["']/.test(line)) bad.push(`${file}:${i + 1}: ${line.trim()}`);
      });
    }
    expect(bad).toEqual([]);
  });

  it("a number field never commits on `input`/`change` by hand", () => {
    // A field with use:numeric must not also carry its own value/onchange,
    // or the two would fight over the typed text.
    const bad: string[] = [];
    let n = 0;
    for (const [file, text] of Object.entries(sources)) {
      for (const m of text.matchAll(/<input\b[\s\S]*?\/>/g)) {
        const tag = m[0];
        const at = tag.indexOf("use:numeric={");
        if (at < 0) continue;
        // Cut the action's own {{ … }} out, braces balanced.
        let depth = 0;
        let end = at + "use:numeric=".length;
        for (; end < tag.length; end++) {
          if (tag[end] === "{") depth++;
          else if (tag[end] === "}" && --depth === 0) break;
        }
        const rest = tag.slice(0, at) + tag.slice(end + 1);
        if (/\s(value|onchange|oninput|bind:value)=/.test(rest)) bad.push(`${file}: ${tag.slice(0, 120)}`);
        n++;
      }
    }
    expect(n).toBeGreaterThan(40);
    expect(bad).toEqual([]);
  });
});
