// Round 2 of the towers audit: the helpers behind the UI fixes.
import { describe, expect, it } from "vitest";
import { i18n } from "./i18n.svelte";
import { describeChange, describeChanges, matchTiers, towerDetails } from "./changes";
import { arcReason, towerRefusal } from "./errors";
import { hasRoofWarning, resultWarnings, towersOnRoof } from "./results";
import { removeTowers, towersOnPart } from "./transform";
import type { CalcResult, Change, Project, Tower, TowerReport } from "./types";

const tier = (rise: number, top_width: number, material = 2) => ({ rise, top_width, material });
const base: Tower = { label: "Zvonice", center: { x: 4, y: 2 }, width: 3, base_height: 9, tiers: [tier(2, 2), tier(0.375, 1.8), tier(1, 0.875), tier(1.4, 0)] };
const modified = (before: Tower, after: Tower) => ({ field: "tower", item: 0, kind: "modified", before, after }) as unknown as Change;

describe("matchTiers (UX-005)", () => {
  it("leaves out the tiers both versions start and end with", () => {
    const a = [1, 2, 3, 4].map((n) => tier(n, 0));
    expect(matchTiers(a, [a[0], a[2], a[3]])).toEqual([[1, null]]);
    expect(matchTiers(a, [...a, tier(9, 0)])).toEqual([[null, 4]]);
    expect(matchTiers(a, a)).toEqual([]);
  });
  it("compares what is left pairwise and adds or removes the rest", () => {
    const a = [1, 2, 3].map((n) => tier(n, 0));
    expect(matchTiers(a, [a[0], tier(7, 0), a[2], tier(9, 0)])).toEqual([[1, 1], [2, 2], [null, 3]]);
  });
});

describe("tower review rows (UX-005)", () => {
  it("one removed tier reads as that tier removed with its values, not as every tier above it changed", () => {
    i18n.locale = "en";
    const after = { ...base, tiers: [base.tiers[0], base.tiers[2], base.tiers[3]] };
    const d = towerDetails(modified(base, after), null);
    expect(d).toHaveLength(1);
    expect(d[0]).toMatch(/^Tier 2 removed: Rise 0\.375 m · Top width 1\.80 m · /);
  });
  it("an added tier lists its values", () => {
    i18n.locale = "en";
    const after = { ...base, tiers: [...base.tiers, tier(1.2, 0, 3)] };
    const d = towerDetails(modified(base, after), null);
    expect(d).toHaveLength(1);
    expect(d[0]).toMatch(/^Tier 5 added: Rise 1\.20 m · Top width 0\.00 m · /);
  });
  it("names the tower by its label and says where it moved from", () => {
    i18n.locale = "en";
    const after = { ...base, center: { x: 4.5, y: 2 } };
    expect(describeChange(modified(base, after), null)).toBe("Changed: Zvonice");
    expect(towerDetails(modified(base, after), null)).toEqual(["moved from x 4.00, y 2.00 to x 4.50, y 2.00"]);
    const unnamed = { ...base, label: "" };
    expect(describeChange(modified(unnamed, unnamed), null)).toBe("Changed: Tower 1");
  });
});

describe("arc review rows (UX-013)", () => {
  it("the corners one save added to a building are one row with their counts", () => {
    i18n.locale = "en";
    const edge = (kind: string, edge: number) => ({ field: "edge", part: 0, edge, kind }) as unknown as Change;
    const rows = describeChanges([edge("added", 1), edge("added", 2), edge("added", 3), edge("removed", 1), { field: "name", after: "X" } as unknown as Change], null);
    expect(rows).toEqual(["Outline changed (points added: 3, removed: 1)", "Name: X"]);
    // One or two corners are still named.
    expect(describeChanges([edge("added", 1)], null)).toEqual([describeChange(edge("added", 1), null)]);
  });
});

describe("towerRefusal (ERR-004, ARCH-002)", () => {
  it("a preset's tower problem is the dialog's note, whether answered as refused or out_of_range", () => {
    i18n.locale = "en";
    expect(towerRefusal({ code: "refused", detail: "a tier rises 0 to 100 m and ends 0 to 200 m wide (tower_tier_size)" })?.text).toMatch(/^A tier rises/);
    expect(towerRefusal({ code: "refused", detail: "too many tiers of all towers: 126 (at most 120) (tower_tiers_limit)" })).toEqual({ text: "All towers of a job have at most 120 tiers together; no more can be added", detail: null });
    expect(towerRefusal({ code: "refused", detail: "x (tower_label)" })?.detail).toBeNull();
  });
  it("an older answer as out_of_range is the dialog's note too", () => {
    i18n.locale = "en";
    const r = towerRefusal({ code: "out_of_range", detail: "a tier rises 0 to 100 m and ends 0 to 200 m wide (tower_tier_size)" });
    expect(r?.detail).toBeNull();
    expect(r?.text).toMatch(/^A tier rises/);
  });
  it("the job's counts (too many tiers in all towers) stay beside OK with the core's text", () => {
    i18n.locale = "en";
    const r = towerRefusal({ code: "too_many", detail: "too many tiers of all towers: 126 (at most 120)" });
    expect(r?.detail).toBe("too_many: too many tiers of all towers: 126 (at most 120)");
  });
  it("a failure of the core is no refusal", () => {
    expect(towerRefusal({ code: "internal", detail: "panicked" })).toBeNull();
    expect(towerRefusal({ code: "out_of_range", detail: "x = 1e9" })).toBeNull();
  });
});

const report = (shaft: Partial<NonNullable<TowerReport["shaft"]>>, label = ""): TowerReport => ({
  label,
  result: { top_height: 12, base_perimeter: 12, base_area: 9, tiers: [], lines: [] },
  roof_area: 10,
  wall_area: 0,
  shaft: { bottom: [], wall_area: 0, on_roof: true, base_below_roof: false, ...shaft },
});
const calc = (towers: TowerReport[], warnings: CalcResult["warnings"] = []) => ({ parts: [], totals: {} as never, codes: {}, problems: [], towers, warnings }) as unknown as CalcResult;

describe("a tower roof below the roof around it (UX-002)", () => {
  it("the core's warning names the tower by its label and says by how much, and marks the Results tab", () => {
    i18n.locale = "en";
    const w = { kind: "tower_base_below_roof" as const, value: 1.2, towers: [0] };
    const r = calc([report({ base_below_roof: true }, "Lantern")]);
    (r as { parts: unknown[] }).parts = [{ name: "Main", geometry: { faces: [], lines: [], gables: [], warnings: [w] }, quantities: {} }];
    expect(resultWarnings(r).map((x) => x.text)).toEqual(["Lantern: the tower roof starts 1.20 m lower than the roof around it, so its lower part lies inside the roof and is still counted in full. Check the height where the tower roof starts."]);
    expect(hasRoofWarning(r)).toBe(true);
    expect(hasRoofWarning(calc([report({})]))).toBe(false);
  });
  it("counts the towers cut out of a roof (their shafts are deducted with the chimneys, MATH-005)", () => {
    expect(towersOnRoof(calc([report({}), report({ on_roof: false })]))).toBe(1);
  });
});

describe("towers deleted with their building (UX-003)", () => {
  const square = (x0: number) => ({ name: "", edges: [[x0, 0], [x0 + 10, 0], [x0 + 10, 8], [x0, 8]].map(([x, y]) => ({ start: { x, y }, pitch_deg: 30, eave_height: 3, overhang: 0.5 })) });
  it("finds the towers whose centre is inside the building and removes them", () => {
    const p = { parts: [square(0), square(20)], towers: [{ ...base, center: { x: 5, y: 4 } }, { ...base, center: { x: 25, y: 4 } }, { ...base, center: { x: 6, y: 1 } }] } as unknown as Project;
    expect(towersOnPart(p, 0)).toEqual([0, 2]);
    expect(towersOnPart(p, 1)).toEqual([1]);
    expect(towersOnPart(p, 5)).toEqual([]);
    removeTowers(p, [2, 0, 0]);
    expect(p.towers!.map((t) => t.center.x)).toEqual([25]);
  });
});

describe("arc refusals by their code (ARCH-005)", () => {
  it("every code of the core has its own text, with the figures of the sentence", () => {
    i18n.locale = "en";
    expect(arcReason("an arc has 2 to 64 walls, not 100 (arc_segments)")).toBe("An arc has 2 to 64 walls.");
    expect(arcReason("the arc's walls would be shorter than 0.15 m: give a larger radius or fewer walls (arc_too_short)")).toContain("0.15 m");
    expect(arcReason("the arc's corners would turn by less than 2°: give a larger bend or fewer walls (arc_flat)")).toContain("less than 2°");
    expect(arcReason("the arc would touch another wall of the building (arc_touches)")).toBe("The arc would touch another wall of the building.");
    expect(arcReason("the radius is too large for the walls at the corner (at most 1.250 m) (arc_radius_max)")).toContain("1.25 m");
    for (const c of ["arc_bulge", "arc_radius_min", "arc_straight", "arc_no_length", "arc_invalid", "arc_no_wall", "arc_no_corner", "arc_no_part"]) {
      expect(arcReason(`reworded (${c})`), c).not.toBeNull();
    }
  });
});
