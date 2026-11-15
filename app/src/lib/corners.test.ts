import { describe, expect, it } from "vitest";
import type { EdgeDef } from "./types";
import { cornerGroup, defaultDirection, onWall, moveCorner, samePoint } from "./corners";

const w = (x: number, y: number, marker = false): EdgeDef => ({
  start: { x, y },
  pitch_deg: 40,
  eave_height: 3,
  overhang: 0.5,
  ...(marker ? { direction_rad: 1 } : {}),
});

describe("corner drag", () => {
  it("moves a direction marker together with its corner", () => {
    // Marker at index 1 sits on corner 2 (10, 0).
    const es = [w(0, 0), w(10, 0, true), w(10, 0), w(10, 6), w(0, 6)];
    expect(cornerGroup(es, 2).sort()).toEqual([1, 2]);
    expect(cornerGroup(es, 1).sort()).toEqual([1, 2]);
    moveCorner(es, 2, 11, 0);
    expect(es[1].start).toEqual({ x: 11, y: 0 });
    expect(es[0].start).toEqual({ x: 0, y: 0 });
  });
});

// Merging a dropped corner and dropping repeated corners are the core's
// (`edit/corners.rs` tests); here only the plan's own one-point test.
describe("one point", () => {
  const p = (x: number, y: number) => ({ x, y });
  it("counts corners a few mm apart as one point and 1 cm as two", () => {
    expect(samePoint(p(1, 1), p(1.0099, 1))).toBe(true);
    expect(samePoint(p(1, 1), p(1.01, 1))).toBe(false);
  });
});

describe("a corner added on a wall", () => {
  const p = (x: number, y: number) => ({ x, y });
  it("lies on a slanted wall, at a grid step from its start (review ERR-002)", () => {
    const a = p(0, 0);
    const b = p(6, 3);
    const q = onWall(a, b, p(2.03, 1.06), 0.05)!;
    // Off the wall by at most the millimetre rounding.
    const off = Math.abs((q.x - a.x) * (b.y - a.y) - (q.y - a.y) * (b.x - a.x)) / Math.hypot(6, 3);
    expect(off).toBeLessThan(0.001);
    expect(Math.hypot(q.x, q.y)).toBeCloseTo(2.3, 2);
  });
  it("stays on an axis wall at grid points and never on its ends", () => {
    expect(onWall(p(0, 3), p(20, 3), p(7.012, 3.04), 0.05)).toEqual(p(7, 3));
    expect(onWall(p(0, 3), p(20, 3), p(-1, 3), 0.05)).toEqual(p(0.05, 3));
    // 2 cm from a corner, zoomed in: never a wall under 1 cm.
    expect(onWall(p(20.003, 3), p(20.003, 9), p(20.003, 3.02), 0.05)).toEqual(p(20.003, 3.05));
    expect(onWall(p(0, 0), p(0.06, 0), p(0.03, 0), 0.05)).toBeNull();
  });
});

describe("default direction of a new direction change (HUNTC-008)", () => {
  it("is the bisector of the previous and this wall, 45 degrees at a right-angled corner", () => {
    const rect = [w(0, 0), w(12, 0), w(12, 8), w(0, 8)];
    expect(defaultDirection(rect, 1)).toBeCloseTo(Math.PI / 4, 9);
    expect(defaultDirection(rect, 2)).toBeCloseTo((3 * Math.PI) / 4, 9);
    expect(defaultDirection(rect, 0)).toBeCloseTo((7 * Math.PI) / 4, 9);
  });
});
