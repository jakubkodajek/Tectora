import { describe, expect, it } from "vitest";
import { DICTS, i18n } from "./i18n.svelte";
import { areaRows, failedParts, joinedWith, roofState, warningText } from "./results";
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
  it("says what Tectora did where the standard calculation stops: cm, m and walls from 1 (TEST-007)", () => {
    const merged: RoofWarning = { kind: "merged_planes", value: 0.0225, edges: [0, 2] };
    const over: RoofWarning = { kind: "plane_over_wall", value: 2.0977, edges: [0, 2] };
    const wing: RoofWarning = { kind: "wing_joined", value: 0, edges: [0, 1, 7] };
    i18n.locale = "cs";
    expect(warningText(merged)).toContain("stěn 1 a 3");
    expect(warningText(merged)).toContain("2,25 cm");
    expect(warningText(over)).toContain("stěny 1");
    expect(warningText(over)).toContain("stěny 3");
    expect(warningText(over)).toContain("2,10 m");
    expect(warningText(wing)).toContain("1, 2, 8");
    i18n.locale = "en";
    expect(warningText(merged)).toContain("walls 1 and 3");
    expect(warningText(merged)).toContain("2.25 cm");
    expect(warningText(wing)).toContain("walls 1, 2, 8");
    for (const locale of Object.keys(DICTS) as (keyof typeof DICTS)[]) {
      i18n.locale = locale;
      for (const w of [merged, over, wing, { kind: "not_calculated_by_core", value: 0 } as RoofWarning]) {
        const text = warningText(w);
        expect(text, `${locale} ${w.kind}`).not.toContain("{");
        expect(text, `${locale} ${w.kind}`).not.toMatch(/dacad|res\.warn/i);
        expect(text.length, `${locale} ${w.kind}`).toBeGreaterThan(30);
      }
    }
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

describe("warningText for every kind", () => {
  // A Record over the union: `npm run check` fails when types.ts gains a kind
  // this list lacks (the core test ui_texts.rs ties types.ts to check.rs).
  const every: Record<RoofWarning["kind"], RoofWarning> = {
    outside_outline: { kind: "outside_outline", value: 0.31 },
    overlapping_faces: { kind: "overlapping_faces", value: 1.25 },
    area_mismatch: { kind: "area_mismatch", value: 0.034 },
    merged_walls: { kind: "merged_walls", value: 0, edges: [1, 2] },
    plane_over_wall: { kind: "plane_over_wall", value: 2.1, edges: [0, 2] },
    merged_planes: { kind: "merged_planes", value: 0.02, edges: [0, 2] },
    wing_joined: { kind: "wing_joined", value: 0, edges: [0, 1] },
    covered: { kind: "covered", value: 0 },
    cut_exceeds_face: { kind: "cut_exceeds_face", value: 3.5, edges: [0] },
    dormer_off_roof: { kind: "dormer_off_roof", value: 0, dormer: 0 },
    window_off_roof: { kind: "window_off_roof", value: 0, item: 0 },
    chimney_off_roof: { kind: "chimney_off_roof", value: 0, item: 0 },
    opening_off_roof: { kind: "opening_off_roof", value: 0, item: 0 },
    not_calculated_by_core: { kind: "not_calculated_by_core", value: 0 },
    doubled_line: { kind: "doubled_line", value: 0.82 },
    far_from_origin: { kind: "far_from_origin", value: 30010 },
    no_roof_area: { kind: "no_roof_area", value: 0 },
    objects_overlap: { kind: "objects_overlap", value: 2 },
    dormer_above_roof: { kind: "dormer_above_roof", value: 9.65 },
    inside_another: { kind: "inside_another", value: 0 },
    merged_materials: { kind: "merged_materials", value: 0, edges: [0, 1] },
  };
  it("gives a filled-in sentence for all 21 kinds in all four languages (TEST-003)", () => {
    expect(Object.keys(every)).toHaveLength(21);
    for (const locale of Object.keys(DICTS) as (keyof typeof DICTS)[]) {
      i18n.locale = locale;
      for (const w of Object.values(every)) {
        expect(DICTS[locale][`res.warn.${w.kind}`], `${locale} ${w.kind}`).toBeTruthy();
        const text = warningText(w);
        expect(text, `${locale} ${w.kind}`).not.toContain("{");
        expect(text, `${locale} ${w.kind}`).not.toMatch(/dacad|res\.warn/i);
        expect(text.length, `${locale} ${w.kind}`).toBeGreaterThan(20);
      }
    }
    i18n.locale = "cs";
  });
  it("puts the value into the sentences that report one", () => {
    i18n.locale = "en";
    expect(warningText(every.outside_outline)).toContain("0.31");
    expect(warningText(every.overlapping_faces)).toContain("1.25");
    expect(warningText(every.area_mismatch)).toContain("3.4");
    expect(warningText(every.merged_walls)).toMatch(/2\b[\s\S]*3\b/);
    expect(warningText(every.merged_materials)).toMatch(/1\b[\s\S]*2\b/);
    i18n.locale = "cs";
  });
});

describe("roofState (ERRU-003)", () => {
  const failing = (name: string) => ({ ...part(name), geometry: { ...part(name).geometry, error: { kind: "open_faces" } } });
  it("is partial when only some buildings have no roof, so the others keep their figures", () => {
    const r = { parts: [part("A"), failing("B")] } as unknown as CalcResult;
    expect(roofState(r)).toBe("partial");
    expect(failedParts(r)).toEqual([1]);
  });
  it("is none when no building has a roof, ok when all have one", () => {
    expect(roofState({ parts: [failing("A")] } as unknown as CalcResult)).toBe("none");
    expect(roofState({ parts: [part("A"), part("B")] } as unknown as CalcResult)).toBe("ok");
    expect(roofState(null)).toBe("ok");
  });
});
