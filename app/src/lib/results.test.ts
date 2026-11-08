import { describe, expect, it } from "vitest";
import { DICTS, i18n } from "./i18n.svelte";
import { joinedWith, warningText } from "./results";
import type { CalcResult, RoofWarning } from "./types";

const part = (name: string, joined_with?: number[]) => ({
  name,
  geometry: { faces: [], lines: [], gables: [], joined_with },
  quantities: {} as CalcResult["parts"][number]["quantities"],
});

describe("joinedWith", () => {
  it("takes the joins from the calculation, not from the plan", () => {
    // Two buildings 0.8 m apart with 0.5 m overhangs: the plan guess called
    // them joined, the calculation does not join them (ARCH-008).
    const apart = { parts: [part("A"), part("B")] } as unknown as CalcResult;
    expect(joinedWith(apart)).toEqual([[], []]);
    const joined = { parts: [part("A", [1]), part("B", [0])] } as unknown as CalcResult;
    expect(joinedWith(joined)).toEqual([[1], [0]]);
    expect(joinedWith(null)).toEqual([]);
  });
});

describe("warningText", () => {
  const kinds: RoofWarning[] = [
    { kind: "cut_exceeds_face", value: 27.34, edges: [0] },
    { kind: "dormer_off_roof", value: 0, dormer: 1 },
    { kind: "not_calculated_by_core", value: 0 },
    { kind: "window_off_roof", value: 0, item: 0 },
    { kind: "chimney_off_roof", value: 0, item: 2 },
    { kind: "opening_off_roof", value: 0, item: 1 },
  ];
  it("names the window, chimney or opening left out, numbered from 1", () => {
    i18n.locale = "en";
    expect(warningText(kinds[3])).toContain("Roof window 1 ");
    expect(warningText(kinds[4])).toContain("Chimney 3 ");
    expect(warningText(kinds[5])).toContain("Opening 2 ");
    i18n.locale = "cs";
    expect(warningText(kinds[3])).toContain("Střešní okno 1 ");
  });
  it("has every new warning in all four languages", () => {
    for (const locale of Object.keys(DICTS) as (keyof typeof DICTS)[]) {
      for (const w of kinds) expect(DICTS[locale][`res.warn.${w.kind}`], `${locale} ${w.kind}`).toBeTruthy();
      expect(DICTS[locale]["opening.tooLarge"], locale).toBeTruthy();
    }
  });
  it("fills in the values", () => {
    i18n.locale = "en";
    expect(warningText(kinds[0])).toContain("27.34");
    expect(warningText(kinds[1])).toContain("Dormer 2");
    expect(warningText(kinds[2])).not.toMatch(/DACAD|core/i);
  });
});
