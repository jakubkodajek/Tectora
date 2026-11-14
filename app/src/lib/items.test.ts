import { describe, expect, it } from "vitest";
import { emptyProject, type CalcResult, type Project } from "./types";
import { deleteItem, followRoof, itemCentre, moveItem, placementOf, rotateItem, transformItem } from "./items";
import { translation } from "./transform";
import { chimneyTop, clipConvex, dormerWallLines, onWallLine, planArea, planeOf } from "./scene3d";

const project = (): Project => {
  const p = emptyProject("T");
  p.chimneys.push({ corners: [{ x: 2, y: 2 }, { x: 3, y: 2 }, { x: 3, y: 3 }, { x: 2, y: 3 }], height: 6 });
  p.windows = [{ x: 5, y: 1, width: 0.78, height: 1.18 }];
  return p;
};

// One roof face over 0..10 x 0..8 rising 0.5 m per metre in y from 3 m.
const roof: CalcResult = {
  parts: [{ name: "A", geometry: { faces: [{ edge: 0, pitch_deg: 26.6, area: 0, polygon: [[0, 0, 3], [10, 0, 3], [10, 8, 7], [0, 8, 7]] }], lines: [], gables: [] }, quantities: {} as never }],
  totals: {} as never,
  codes: {},
  problems: [],
};

describe("single items (as tectora-core transform_items)", () => {
  it("moves one chimney and keeps its top", () => {
    const p = project();
    moveItem(p, { kind: "chimney", index: 0 }, 1, -0.5);
    expect(p.chimneys[0].corners[0]).toEqual({ x: 3, y: 1.5 });
    expect(p.chimneys[0].height).toBe(6);
    expect(p.windows![0]).toEqual({ x: 5, y: 1, width: 0.78, height: 1.18 });
  });

  it("rotates about the item's centre and copies append", () => {
    const p = project();
    const it = { kind: "chimney" as const, index: 0 };
    const out = rotateItem(p, it, 45, 2);
    expect(out.map((r) => r.index)).toEqual([1, 2]);
    expect(itemCentre(p, out[0])).toEqual({ x: 2.5, y: 2.5 });
    const xs = p.chimneys[2].corners.map((q) => q.x).sort();
    expect([xs[0], xs[3]]).toEqual([2, 3]);
  });

  it("a duplicate is a separate object", () => {
    const p = project();
    const [c] = transformItem(p, { kind: "window", index: 0 }, translation(0.5, -0.5), 1);
    expect(c).toEqual({ kind: "window", index: 1 });
    p.windows![1].width = 2;
    expect(p.windows![0].width).toBe(0.78);
    deleteItem(p, { kind: "window", index: 0 });
    expect(p.windows!.map((w) => w.width)).toEqual([2]);
  });

  it("follows the roof: a chimney moved 1 m up the slope rises 0.5 m", () => {
    const p = project();
    const it = { kind: "chimney" as const, index: 0 };
    moveItem(p, it, 0, 1);
    followRoof(p, it, { x: 2.5, y: 2.5 }, { x: 2.5, y: 3.5 }, roof);
    expect(p.chimneys[0].height).toBe(6.5);
  });
});

describe("3D scene geometry", () => {
  it("reads a face's plane height", () => {
    const z = planeOf(roof.parts[0].geometry.faces[0].polygon)!;
    expect(z({ x: 4, y: 2 })).toBeCloseTo(4);
  });

  it("clips a face to a convex opening", () => {
    const face = [{ x: 0, y: 0 }, { x: 10, y: 0 }, { x: 10, y: 4 }, { x: 0, y: 4 }];
    const hole = [{ x: 1, y: 3 }, { x: 2, y: 3 }, { x: 2, y: 5 }, { x: 1, y: 5 }];
    expect(planArea(clipConvex(face, hole))).toBeCloseTo(1);
    expect(planArea(clipConvex(face, [...hole].reverse()))).toBeCloseTo(1);
  });

  it("finds roof edges on a dormer's cheeks and front", () => {
    const lines = dormerWallLines({
      edges: [
        { start: { x: 3, y: 1.5 }, pitch_deg: 45, eave_height: 5.69, overhang: 0, direction_rad: (3 * Math.PI) / 2 },
        { start: { x: 3, y: 1.5 }, pitch_deg: 90, eave_height: 5.69, overhang: 0 },
        { start: { x: 5, y: 1.5 }, pitch_deg: 45, eave_height: 5.69, overhang: 0, direction_rad: Math.PI / 2 },
      ],
      end: { x: 5, y: 1.5 },
    });
    // Cheek on x = 3: the dormer roof runs towards +y, its marker points -y.
    expect(onWallLine(lines, [3, 1.5, 5.69], [3, 2.71, 5.69])).toBe(true);
    expect(onWallLine(lines, [4, 1.5, 6.69], [3, 1.5, 5.69])).toBe(true);
    expect(onWallLine(lines, [4, 3.9, 6.69], [4, 1.5, 6.69])).toBe(false);
  });

  it("draws a chimney with no top 1 m above the roof", () => {
    const c = { corners: [{ x: 0, y: 0 }, { x: 1, y: 0 }, { x: 1, y: 1 }], height: 0 };
    expect(chimneyTop(c, (p) => 4 + p.y)).toEqual({ bottom: 3.7, top: 6 });
    expect(chimneyTop({ ...c, height: 8 }, (p) => 4 + p.y).top).toBe(8);
  });
});

describe("placementOf", () => {
  const pl = (from_eave: number) => ({
    part: 0, edge: 0, pitch_deg: 40, from_eave, from_eave_plan: from_eave, from_left: 1, from_right: 2,
    eave_line: [{ x: 0, y: 0 }, { x: 1, y: 0 }], left_line: [{ x: 0, y: 0 }, { x: 0, y: 1 }], right_line: [{ x: 1, y: 0 }, { x: 1, y: 1 }],
  });
  const result = { placements: { dormers: [pl(1)], chimneys: [pl(2)], windows: [null, pl(3)], roof_openings: [pl(4)] } } as unknown as CalcResult;

  it("finds each kind's distances by its index", () => {
    expect(placementOf(result, { kind: "dormer", index: 0 })?.from_eave).toBe(1);
    expect(placementOf(result, { kind: "chimney", index: 0 })?.from_eave).toBe(2);
    expect(placementOf(result, { kind: "window", index: 1 })?.from_eave).toBe(3);
    expect(placementOf(result, { kind: "hole", index: 0 })?.from_eave).toBe(4);
  });

  it("has none off the roof, for other objects or without a result", () => {
    expect(placementOf(result, { kind: "window", index: 0 })).toBeNull();
    expect(placementOf(result, { kind: "window", index: 7 })).toBeNull();
    expect(placementOf(result, { kind: "line", index: 0 } as never)).toBeNull();
    expect(placementOf(null, { kind: "dormer", index: 0 })).toBeNull();
  });
});
