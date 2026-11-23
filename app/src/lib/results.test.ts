import { describe, expect, it } from "vitest";
import { DICTS, i18n } from "./i18n.svelte";
import { areaRows, failedParts, hasRoofWarning, joinedWith, objectCounts, offRoofWarning, roofState, warningText } from "./results";
import type { CalcResult, Project, RoofWarning } from "./types";

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
    hidden_under_neighbour: { kind: "hidden_under_neighbour", value: 2.4 },
    uncovered_plan: { kind: "uncovered_plan", value: 15.3 },
  };
  it("gives a filled-in sentence for all 22 kinds in all four languages (TEST-003)", () => {
    expect(Object.keys(every)).toHaveLength(22);
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
  it("counts a building left out for its input as without roof (ERR-001, UI-004)", () => {
    // A pitch of 1° or a doubled corner: the core leaves the part out, so
    // it has no error, only problems, and its zeros are no figures.
    const left = { parts: [part("A"), part("B")], problems: [{ item: "part", index: 1, edge: 0, code: "pitch_range" }] };
    expect(roofState(left as unknown as CalcResult)).toBe("partial");
    expect(failedParts(left as unknown as CalcResult)).toEqual([1]);
    const alone = { parts: [part("A")], problems: [{ item: "part", index: 0, code: "too_few_edges" }] };
    expect(roofState(alone as unknown as CalcResult)).toBe("none");
    // A refused chimney leaves no building without its roof.
    const chimney = { parts: [part("A")], problems: [{ item: "chimney", index: 0, code: "bad_size" }] };
    expect(roofState(chimney as unknown as CalcResult)).toBe("ok");
  });
});

describe("an empty job shows no zero figures (UX-011)", () => {
  it("is empty with no building and no hand-drawn face", async () => {
    const { emptyRoof } = await import("./results");
    const base = { parts: [], totals: {}, codes: {}, problems: [] } as unknown as CalcResult;
    expect(emptyRoof(null)).toBe(true);
    expect(emptyRoof(base)).toBe(true);
    expect(emptyRoof({ ...base, manual_faces: [{ label: "1", pitch_deg: 30, area: 4 }] })).toBe(false);
    expect(emptyRoof({ ...base, parts: [{}] } as unknown as CalcResult)).toBe(false);
  });
});

describe("no result for a job with buildings is not an empty job (UX-101)", () => {
  it("says the calculation runs or failed, not that there is no building", async () => {
    const { missingResult } = await import("./results");
    const built = { parts: [{}], manual_faces: [] } as never;
    const empty = { parts: [], manual_faces: [] } as never;
    const fail = { code: "too_complex", title: "x", message: "too complex", detail: "" };
    expect(missingResult(built, null, null)).toBe("calculating");
    expect(missingResult(built, null, fail)).toBe("failed");
    expect(missingResult(empty, null, fail)).toBe("empty");
    expect(missingResult(empty, null, null)).toBe("empty");
    const r = { parts: [{}], totals: {}, codes: {}, problems: [] } as unknown as CalcResult;
    expect(missingResult(built, r, null)).toBeNull();
    // Without the job, the result decides as before.
    expect(missingResult(null, null, null)).toBe("empty");
  });
});

describe("a roof of hand-drawn faces only (UX-302)", () => {
  it("is a job with a face and no building", async () => {
    const { isFacesOnly } = await import("./results");
    expect(isFacesOnly({ parts: [], manual_faces: [{ points: [], pitch_deg: 30 }] })).toBe(true);
    expect(isFacesOnly({ parts: [{ name: "A", edges: [] }], manual_faces: [{ points: [], pitch_deg: 30 }] })).toBe(false);
    expect(isFacesOnly({ parts: [] })).toBe(false);
    expect(isFacesOnly(null)).toBe(false);
  });

  it("lists no line rows of zero, and every row for a building", async () => {
    const { lineRowsShown } = await import("./results");
    const rows = [{ code: "T", value: 0 }, { code: "F", value: 0.004 }, { code: "O", value: 6 }];
    expect(lineRowsShown(rows, true).map((r) => r.code)).toEqual(["O"]);
    expect(lineRowsShown(rows, false)).toHaveLength(3);
  });
});

describe("texts fixed in the round-4 wording group (UX-302)", () => {
  const sources = import.meta.glob(["./Results.svelte", "./AiPanel.svelte"], { query: "?raw", import: "default", eager: true }) as Record<string, string>;
  it("the calculation steps' intro names the dormers the face rows deduct", () => {
    for (const [lang, word] of [["cs", "vikýře"], ["sk", "vikiere"], ["en", "dormers"], ["de", "Gauben"]] as const) expect(DICTS[lang]["bd.intro"]).toContain(word);
  });
  it("the doubled-lines hint names no cause the job may not have", () => {
    for (const [lang, word] of [["cs", "různou výškou"], ["sk", "rôznou výškou"], ["en", "different eave heights"], ["de", "unterschiedlicher Traufhöhe"]] as const)
      expect(DICTS[lang]["res.warn.doubled_line"]).not.toContain(word);
  });
  it("the AI tab's settings history has one colon, as the card", () => {
    expect(sources["./AiPanel.svelte"]).not.toContain("{rowLabel(r)}:");
  });
  it("the figure of a recalculation on its way is --text-2 on the hero (UI-305)", () => {
    expect(sources["./Results.svelte"]).toMatch(/\.results\.pending \.hero strong \{\s*color: var\(--text-2\);/);
  });
});

describe("an object placed on no roof face (audit 2026-11-23 UX-1401)", () => {
  const warned = (...warnings: RoofWarning[][]) =>
    ({ parts: warnings.map((w, i) => ({ ...part(`P${i}`), geometry: { ...part(`P${i}`).geometry, warnings: w } })) }) as unknown as CalcResult;
  it("finds the core's off-roof warning of the placed item in any building, nothing else", () => {
    const r = warned([{ kind: "chimney_off_roof", value: 0, item: 0 }], [{ kind: "window_off_roof", value: 0, item: 2 }]);
    expect(offRoofWarning(r, "window", 2)?.kind).toBe("window_off_roof");
    expect(offRoofWarning(r, "window", 1)).toBeNull();
    expect(offRoofWarning(r, "chimney", 0)?.kind).toBe("chimney_off_roof");
    expect(offRoofWarning(r, "opening", 0)).toBeNull();
    expect(offRoofWarning(warned([{ kind: "opening_off_roof", value: 0, item: 3 }]), "opening", 3)?.item).toBe(3);
    expect(offRoofWarning(null, "window", 0)).toBeNull();
  });
  it("marks the Results tab for a warning, not for a notice of the core", () => {
    expect(hasRoofWarning(warned([{ kind: "window_off_roof", value: 0, item: 0 }]))).toBe(true);
    expect(hasRoofWarning(warned([], [{ kind: "overlapping_faces", value: 1 }]))).toBe(true);
    for (const kind of ["plane_over_wall", "merged_planes", "wing_joined", "far_from_origin"] as const) {
      expect(hasRoofWarning(warned([{ kind, value: 1 }])), kind).toBe(false);
    }
    expect(hasRoofWarning(warned([]))).toBe(false);
    expect(hasRoofWarning(null)).toBe(false);
  });
});

describe("objectCounts (audit 2026-11-23 UX-1503)", () => {
  const win = (x: number) => ({ x, y: 0, width: 0.78, height: 1.18 });
  const sq = { corners: [{ x: 0, y: 0 }, { x: 1, y: 0 }, { x: 1, y: 1 }], height: 1 };
  const job = { windows: [win(1), win(2)], chimneys: [sq, sq], roof_openings: [sq], dormers: [] } as unknown as Project;
  const result = (warnings: RoofWarning[]) =>
    ({ parts: [{ ...part("A"), geometry: { ...part("A").geometry, warnings } }] }) as unknown as CalcResult;
  it("takes what the core reported off the roof out of the counts on the roof", () => {
    const c = objectCounts(job, result([{ kind: "window_off_roof", value: 0, item: 1 }, { kind: "opening_off_roof", value: 0, item: 0 }]));
    expect(c.windows).toEqual([win(1)]);
    expect([c.chimneys, c.holes, c.dormers]).toEqual([2, 0, 0]);
    expect(c.off).toEqual({ windows: 1, chimneys: 0, holes: 1, dormers: 0 });
    i18n.locale = "en";
    expect(DICTS.en["res.obj.windows"].replace("{n}", String(c.windows.length))).toBe("Roof windows: 1");
    expect(DICTS.en["res.offRoof"].replace("{n}", String(c.off.windows))).toBe("off the roof: 1");
    for (const l of ["cs", "sk", "en", "de"] as const) expect(DICTS[l]["res.offRoof"], l).toContain("{n}");
  });
  it("counts everything on the roof without a result, and ignores an index the job does not have", () => {
    expect(objectCounts(job, null).off).toEqual({ windows: 0, chimneys: 0, holes: 0, dormers: 0 });
    expect(objectCounts(job, result([{ kind: "chimney_off_roof", value: 0, item: 7 }])).chimneys).toBe(2);
  });
});
