import { describe, expect, it } from "vitest";
import { DICTS, i18n } from "./i18n.svelte";
import { areaRows, joinedWith, warningText } from "./results";
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

describe("areaRows", () => {
  it("lists the hand-drawn faces after the parts, and the rows add up to the total (UX-009)", () => {
    i18n.locale = "cs";
    const withArea = (name: string, roof_area: number) => ({ ...part(name), quantities: { roof_area } as CalcResult["parts"][number]["quantities"] });
    const result = {
      parts: [withArea("Hlavní", 152.73), withArea("Křídlo", 20.5)],
      manual_faces: [
        { label: "Stříška", pitch_deg: 60, area: 6 },
        { label: "", pitch_deg: 0, area: 8 },
      ],
      totals: { roof_area: 187.23 },
    } as unknown as CalcResult;
    const rows = areaRows(result);
    expect(rows.map((r) => r.name)).toEqual(["Hlavní", "Křídlo", "Stříška", "Plocha 2"]);
    expect(rows.map((r) => r.manual)).toEqual([false, false, true, true]);
    const sum = rows.reduce((s, r) => s + r.area, 0);
    expect(sum).toBeCloseTo(result.totals.roof_area, 9);
    // One part and one face: two rows, so the list is shown.
    const one = { parts: [withArea("A", 10)], manual_faces: [{ label: "", pitch_deg: 30, area: 2 }], totals: { roof_area: 12 } } as unknown as CalcResult;
    expect(areaRows(one).length).toBe(2);
    // Older results without the field: the parts only.
    const old = { parts: [withArea("A", 10)], totals: { roof_area: 10 } } as unknown as CalcResult;
    expect(areaRows(old).map((r) => r.name)).toEqual(["A"]);
    expect(areaRows(null)).toEqual([]);
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
