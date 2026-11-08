import { describe, expect, it } from "vitest";
import { MAX_GRID_LINES, fitScale, gridStep, gridValues } from "./grid";

describe("gridStep", () => {
  it("keeps the 0.5 m grid and its doubled step at normal zoom", () => {
    expect(gridStep(40, 0.5)).toBe(0.5);
    expect(gridStep(20, 0.5)).toBe(1);
    expect(gridStep(10, 0.5)).toBe(5);
  });

  it("never draws lines closer than 8 px, however far out the view is (UI-011)", () => {
    // A 2000 × 1000 m part fits a 1000 px view at about 0.4 px per m.
    for (const scale of [1, 0.4, 0.05, 0.001]) {
      const s = gridStep(scale, 0.5);
      expect(scale * s, `scale ${scale}`).toBeGreaterThanOrEqual(8);
      // A power of ten of the base step, so major lines stay on round metres.
      expect(s / 0.5, `scale ${scale}`).toBeCloseTo(10 ** Math.round(Math.log10(s / 0.5)), 6);
    }
  });

  it("falls back to a coarse step for a zoom that is not a number", () => {
    expect(gridStep(0, 0.5)).toBeGreaterThan(0);
    expect(gridStep(NaN, 0.5)).toBeGreaterThan(0);
  });
});

describe("gridValues", () => {
  it("lists the multiples of the step in the range", () => {
    expect(gridValues(-1.2, 1, 0.5)).toEqual([-1.5, -1, -0.5, 0, 0.5, 1]);
  });

  it("is empty for infinite bounds, a zero step or too many lines (UI-001)", () => {
    expect(gridValues(-Infinity, Infinity, 5)).toEqual([]);
    expect(gridValues(0, NaN, 5)).toEqual([]);
    expect(gridValues(0, 10, 0)).toEqual([]);
    expect(gridValues(0, MAX_GRID_LINES * 10, 1)).toEqual([]);
  });
});

describe("fitScale", () => {
  it("fits the plan into the view", () => {
    expect(fitScale(800, 400, 10, 5)).toBe(60);
  });

  it("refuses a view with no room instead of a zoom of 0 (UI-001)", () => {
    expect(fitScale(800, 0, 10, 5)).toBeNull();
    expect(fitScale(5, 400, 10, 5)).toBeNull();
  });
});
