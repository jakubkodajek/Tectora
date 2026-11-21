/// <reference types="vite/client" />
import { describe, expect, it } from "vitest";
import { detailOf, lineTitle, sameLine } from "./lineinfo";
import { DICTS, i18n } from "./i18n.svelte";
import type { CalcResult, LineDetail } from "./types";

const line = (part: number, index: number, code = "F"): LineDetail => ({
  part,
  index,
  kind: "ridge",
  code,
  a: [0, 0, 6],
  b: [4, 0, 6],
  length: 4,
  plan_length: 4,
  slope_deg: 0,
  z_low: 6,
  z_high: 6,
  faces: [],
});

describe("line details", () => {
  it("names every line code with a singular name in every language", () => {
    for (const loc of Object.keys(DICTS) as (keyof typeof DICTS)[]) {
      i18n.locale = loc;
      for (const c of ["T", "F", "G", "K", "O", "OL", "OR", "OS", "P", "c", "S", "W", "WV", "WS", "WH"]) {
        const name = lineTitle(c);
        expect(name, `${loc} ${c}`).not.toMatch(/^lineinfo\.|^line\.|^bd\./);
        expect(name, `${loc} ${c}`).not.toBe(DICTS[loc]["line.other"]);
      }
      expect(lineTitle("zz")).toBe(DICTS[loc]["line.other"]);
    }
    i18n.locale = "cs";
    expect(lineTitle("F")).toBe("Hřeben");
    expect(lineTitle("T")).toBe("Okap");
  });

  it("finds a line's details by part and index", () => {
    const result = { line_details: [line(0, 0), line(0, 1, "G"), line(1, 0, "T")] } as unknown as CalcResult;
    expect(detailOf(result, { part: 0, index: 1 })?.code).toBe("G");
    expect(detailOf(result, { part: 1, index: 0 })?.code).toBe("T");
    expect(detailOf(result, { part: 2, index: 0 })).toBeNull();
    expect(detailOf(null, { part: 0, index: 0 })).toBeNull();
    expect(detailOf(result, null)).toBeNull();
    // A result without details (older bridge) shows nothing rather than failing.
    expect(detailOf({} as CalcResult, { part: 0, index: 0 })).toBeNull();
    expect(sameLine({ part: 0, index: 1 }, { part: 0, index: 1 })).toBe(true);
    expect(sameLine({ part: 0, index: 1 }, { part: 1, index: 1 })).toBe(false);
    expect(sameLine(null, { part: 0, index: 1 })).toBe(false);
  });
});
