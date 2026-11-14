import { describe, expect, it } from "vitest";
import type { EdgeDef } from "./types";
import { cornerGroup, mergeDroppedCorner, moveCorner, samePoint, withoutRepeats } from "./corners";

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

  it("merges a corner dropped on its neighbour, keeping three walls", () => {
    const es = [w(0, 0), w(5, 0), w(10, 0), w(10, 6), w(0, 6)];
    es[1].start = { x: 10.002, y: 0 };
    expect(mergeDroppedCorner(es, 1)).toBe(true);
    expect(es.map((e) => e.start.x)).toEqual([0, 10, 10, 0]);
    const tri = [w(0, 0), w(10, 0), w(0, 6)];
    tri[1].start = { x: 0, y: 6 };
    expect(mergeDroppedCorner(tri, 1)).toBe(false);
    expect(tri).toHaveLength(3);
  });

  it("leaves a corner that is only near, not on, its neighbour", () => {
    const es = [w(0, 0), w(9.9, 0), w(10, 0), w(10, 6), w(0, 6)];
    expect(mergeDroppedCorner(es, 1)).toBe(false);
  });
});

describe("drawn footprint without repeated corners", () => {
  const p = (x: number, y: number) => ({ x, y });
  it("drops a corner clicked twice and a closing corner on the first one", () => {
    // A wall of no length stopped the roof of every building (zero_length_wall).
    expect(withoutRepeats([p(0, 0), p(8, 0), p(8, 0), p(8, 6), p(0, 6), p(0, 0)])).toEqual([p(0, 0), p(8, 0), p(8, 6), p(0, 6)]);
    expect(withoutRepeats([p(0, 0), p(8, 0), p(8, 6), p(0.0004, 0)])).toEqual([p(0, 0), p(8, 0), p(8, 6)]);
  });
  it("keeps corners 1 mm apart and more", () => {
    const pts = [p(0, 0), p(0.001, 0), p(8, 6)];
    expect(withoutRepeats(pts)).toEqual(pts);
    expect(samePoint(p(1, 1), p(1.0009, 1))).toBe(true);
  });
});
