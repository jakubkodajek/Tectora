/// <reference types="vite/client" />
import { describe, expect, it } from "vitest";
import { STEP_WAIT_MS, formatNumber, liveRestore, parseNumber, stepValue, toSend, waitingStep } from "./numeric.svelte";

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

  it("no hand-made number field: a text input read as a number goes through use:numeric", () => {
    // A plain text field marked as numeric, or whose bound text is parsed
    // with parseFloat / Number, takes "5abc" as 5 and "1 250" as 1 while
    // every use:numeric field refuses the first and reads 1250 (UI-004).
    const bad: string[] = [];
    for (const [file, text] of Object.entries(sources)) {
      for (const m of text.matchAll(/<input\b[\s\S]*?\/>/g)) {
        const tag = m[0];
        if (tag.includes("use:numeric")) continue;
        if (/inputmode=["']?(decimal|numeric)/i.test(tag)) bad.push(`${file}: ${tag.slice(0, 120)}`);
        const bound = tag.match(/bind:value=\{([\w.]+)\}/)?.[1];
        if (bound && new RegExp(`(parseFloat|parseInt|Number)\\(\\s*${bound.replace(/\./g, "\\.")}\\b`).test(text)) {
          bad.push(`${file}: ${tag.slice(0, 120)} (parsed by hand)`);
        }
      }
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

describe("waitingStep (UI-002, UI-201)", () => {
  const burst = { from: 2, to: 2.15, at: 1000 };
  it("goes on from the last step while none came back", () => {
    expect(waitingStep(burst, 2, 1100)).toBe(2.15);
  });
  it("still goes on from the last step when an earlier step of the burst came back", () => {
    expect(waitingStep(burst, 2.05, 1100)).toBe(2.15);
    expect(waitingStep(burst, 2.1, 1100)).toBe(2.15);
  });
  it("goes on from the stored value once the last step is there, after a while, or after an outside change", () => {
    expect(waitingStep(burst, 2.15, 1100)).toBeNull();
    expect(waitingStep(burst, 2, 1000 + STEP_WAIT_MS)).toBeNull();
    expect(waitingStep(burst, 3, 1100)).toBeNull();
    expect(waitingStep(null, 2, 1100)).toBeNull();
  });
});

describe("toSend (UI-307)", () => {
  it("sends a value that differs from the stored one, and not the stored one again", () => {
    expect(toSend(3.05, 3, null)).toBe(true);
    expect(toSend(3, 3, null)).toBe(false);
  });
  it("up then down while the first step is out sends the start value after it", () => {
    // 3 stored, 3.05 with the core, the user steps back to 3.
    expect(toSend(3, 3, 3.05)).toBe(true);
  });
  it("the old value typed back while the new one is out is sent", () => {
    expect(toSend(2.05, 2.05, 2.6)).toBe(true);
  });
  it("the value already on its way is not sent twice", () => {
    expect(toSend(3.05, 3, 3.05)).toBe(false);
  });
});

describe("liveRestore (ERR-001)", () => {
  it("puts back the value from before the typing, not a prefix of the refused text", () => {
    // 3 stored, "3000" typed key by key: 3, 30 stored live, then 300 refused.
    expect(liveRestore(3, 30)).toEqual({ v: 3 });
    // Escape after "250" typed over 2.25: 25 was stored live.
    expect(liveRestore(2.25, 25)).toEqual({ v: 2.25 });
  });
  it("puts nothing back when nothing else was stored while typing", () => {
    expect(liveRestore(3, null)).toBeNull();
    expect(liveRestore(3, 3)).toBeNull();
  });
  it("asks to clear a field that started empty", () => {
    expect(liveRestore(null, 10)).toEqual({ v: null });
    expect(liveRestore(undefined, 10)).toEqual({ v: null });
  });
});
