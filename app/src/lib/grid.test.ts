import { describe, expect, it } from "vitest";
import { MAX_GRID_LINES, fitScale, gridValues } from "./grid";

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
