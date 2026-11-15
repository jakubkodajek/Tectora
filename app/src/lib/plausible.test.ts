import { describe, expect, it } from "vitest";
import { aboveRidge, aboveRidgeIssues, breakDefault, halfHipDefault, inputWarnings, isUnusual, ridgeOf, unusualValues } from "./plausible";
import { i18n } from "./i18n.svelte";
import type { CalcResult, EdgeDef, Project } from "./types";

const edge = (x: number, y: number, more: Partial<EdgeDef> = {}): EdgeDef => ({ start: { x, y }, pitch_deg: 30, eave_height: 3, overhang: 0.5, ...more });
const rect = (more: (i: number) => Partial<EdgeDef> = () => ({})): Pick<Project, "parts"> => ({
  parts: [{ name: "Main", edges: [edge(0, 0, more(0)), edge(10, 0, more(1)), edge(10, 6, more(2)), edge(0, 6, more(3))] }],
});
/** A calculated roof whose ridge lies at `ridge` m. */
const result = (ridge: number): CalcResult =>
  ({
    parts: [{ name: "Main", geometry: { faces: [{ edge: 0, pitch_deg: 30, polygon: [], area: 40 }], lines: [], gables: [] }, quantities: { ridge_height: ridge } }],
  }) as unknown as CalcResult;

describe("unusual values (UX-009)", () => {
  it("marks an overhang over 3 m, a height over 30 m and a wall over 200 m", () => {
    expect(isUnusual("overhang", 5)).toBe(true);
    expect(isUnusual("overhang", 0.5)).toBe(false);
    expect(isUnusual("height", 300)).toBe(true);
    expect(isUnusual("height", 12)).toBe(false);
    expect(isUnusual("length", 250)).toBe(true);
    expect(isUnusual("length", null)).toBe(false);
  });

  it("finds the wall with 5 m of overhang typed for 0,5", () => {
    const p = rect((i) => (i === 1 ? { overhang: 5 } : {}));
    expect(unusualValues(p)).toEqual([{ part: 0, edge: 1, field: "overhang", value: 5 }]);
    expect(unusualValues(rect())).toEqual([]);
  });

  it("says it in the Results in every language", () => {
    const p = rect((i) => (i === 1 ? { overhang: 5 } : {}));
    for (const l of ["cs", "sk", "en", "de"] as const) {
      i18n.locale = l;
      const w = inputWarnings(p, null);
      expect(w).toHaveLength(1);
      expect(w[0].text).toContain("2");
      expect(w[0].text).toContain("5");
    }
    i18n.locale = "cs";
  });
});

describe("half-hip and break above the ridge (UX-003)", () => {
  it("starts a new half-hip halfway between eave and ridge", () => {
    // Gable 10 × 6 m, 30°, eave 3 m: ridge 5,02 m; the old default 5,5 m changed nothing.
    expect(halfHipDefault({ eave_height: 3 }, 5.02)).toBe(4.01);
    expect(aboveRidge(halfHipDefault({ eave_height: 3 }, 5.02), 5.02)).toBe(false);
    expect(halfHipDefault({ eave_height: 3 }, null)).toBe(5.5);
  });

  it("starts a new break below the ridge", () => {
    expect(breakDefault(3, 10)).toBe(4.5);
    expect(breakDefault(3, 4)).toBe(3.5);
    expect(breakDefault(3, null)).toBe(4.5);
  });

  it("finds a half-hip at or above the ridge, and the ridge only from a calculated roof", () => {
    const gables = (h: number) => rect((i) => (i % 2 ? { pitch_deg: 90, half_hip: { height: h, pitch_deg: 50 } } : {}));
    expect(aboveRidgeIssues(gables(5.5), result(5.02)).map((r) => [r.edge, r.brk])).toEqual([
      [1, -1],
      [3, -1],
    ]);
    expect(aboveRidgeIssues(gables(4.5), result(5.02))).toEqual([]);
    expect(aboveRidgeIssues(gables(5.5), null)).toEqual([]);
    expect(ridgeOf(result(5.02), 0)).toBe(5.02);
    expect(ridgeOf(result(5.02), 3)).toBeNull();
  });

  it("finds a pitch break above the ridge", () => {
    const p = rect((i) => (i === 0 ? { breaks: [{ height: 4, pitch_deg: 20 }, { height: 9, pitch_deg: 10 }] } : {}));
    expect(aboveRidgeIssues(p, result(6)).map((r) => [r.edge, r.brk])).toEqual([[0, 1]]);
    i18n.locale = "en";
    expect(inputWarnings(p, result(6))[0].text).toContain("break 2 of wall 1");
    i18n.locale = "cs";
  });
});
