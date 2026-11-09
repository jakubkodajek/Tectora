import { describe, expect, it } from "vitest";
import { joinedParts, splitByCover } from "./planjoin";
import type { RoofPart } from "./types";

const rect = (x0: number, y0: number, x1: number, y1: number, overhang = 0.5): RoofPart => ({
  name: "",
  edges: [
    [x0, y0],
    [x1, y0],
    [x1, y1],
    [x0, y1],
  ].map(([x, y]) => ({ start: { x, y }, pitch_deg: 40, eave_height: 3, overhang })),
});

describe("splitByCover", () => {
  it("keeps a line nothing covers in one piece", () => {
    const p = splitByCover([0, 0, 3], [10, 0, 3], () => null);
    expect(p).toEqual([{ a: [0, 0, 3], b: [10, 0, 3], hidden: false }]);
  });

  it("dashes the stretch under a higher roof", () => {
    // A ridge at 4 m whose right half lies under a roof face at 6 m.
    const p = splitByCover([0, 0, 4], [12, 0, 4], (q) => (q.x > 6 ? 6 : 4));
    expect(p.map((x) => x.hidden)).toEqual([false, true]);
    expect(p[0].b[0]).toBeCloseTo(6, 9);
    expect(p[1].b).toEqual([12, 0, 4]);
  });

  it("ignores the face the line lies on", () => {
    const p = splitByCover([0, 0, 4], [12, 0, 4], () => 4.02);
    expect(p.every((x) => !x.hidden)).toBe(true);
  });
});

describe("joinedParts", () => {
  it("joins buildings whose overhangs reach each other", () => {
    expect(joinedParts([rect(0, 0, 12, 8), rect(12.3, 1, 15.3, 7)])).toEqual([[1], [0]]);
  });

  it("joins overlapping and nested buildings", () => {
    expect(joinedParts([rect(0, 0, 12, 8), rect(11, 1, 14, 7)])).toEqual([[1], [0]]);
    expect(joinedParts([rect(0, 0, 12, 8), rect(4, 2, 7, 5)])).toEqual([[1], [0]]);
  });

  it("leaves separate buildings apart", () => {
    expect(joinedParts([rect(0, 0, 12, 8), rect(20, 1, 23, 7), rect(13.5, 0, 15, 2)])).toEqual([[], [], []]);
  });
});
