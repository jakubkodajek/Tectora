import { describe, expect, it } from "vitest";
import { ShapeUtils, Vector2 } from "three";
import { dormerFaces, dormerWallLines, crossings, gableContour, manualFacePoly, outgrows, planArea, wallTriangles, wallTopAt, wallUnderRoof, gableShownByWall, inwardNormal, type Poly3 } from "./scene3d";
import { hostHeightAt } from "./dormers";
import type { CalcResult } from "./types";

/** Area of a planar 3D polygon (half the Newell normal's length). */
function area3(poly: Poly3): number {
  let [x, y, z] = [0, 0, 0];
  for (let i = 0; i < poly.length; i++) {
    const [a, b] = [poly[i], poly[(i + 1) % poly.length]];
    x += a[1] * b[2] - a[2] * b[1];
    y += a[2] * b[0] - a[0] * b[2];
    z += a[0] * b[1] - a[1] * b[0];
  }
  return Math.hypot(x, y, z) / 2;
}

/** Area of the 3D triangles `idx` over `poly`. */
function trianglesArea(poly: Poly3, idx: number[]): number {
  let s = 0;
  for (let i = 0; i < idx.length; i += 3) s += area3([poly[idx[i]], poly[idx[i + 1]], poly[idx[i + 2]]]);
  return s;
}

describe("wallTriangles (what Roof3D draws for a gable, TEST-004)", () => {
  it("covers a gable that starts with a vertical edge exactly, every triangle in the wall plane", () => {
    // The core's gable of the first roof once a second, lower roof joins
    // it (Sedlo plus a building at x 12.3): it starts with a vertical edge.
    const poly: Poly3 = [
      [12.5, 1.2, 3.0],
      [12.5, 1.2, 3.59],
      [12.5, 6.8, 3.59],
      [12.5, 7.0, 3.42],
      [12.5, 7.0, 3.0],
      [12.5, 8.5, 3.0],
      [12.5, 4.0, 6.78],
      [12.5, -0.5, 3.0],
    ];
    const idx = wallTriangles(poly)!;
    expect(idx).not.toBeNull();
    expect(idx.length % 3).toBe(0);
    expect(trianglesArea(poly, idx)).toBeCloseTo(area3(poly), 6);
    for (const i of idx) expect(poly[i][0]).toBe(12.5);
  });

  it("refuses a polygon with no horizontal extent", () => {
    expect(wallTriangles([[1, 1, 0], [1, 1, 2], [1, 1, 3]])).toBeNull();
  });
});

describe("wallUnderRoof (walls close the building under the roof)", () => {
  // A shed roof over 0..10 × 0..6: eave on y = 0 at 3 m, rising 0.5 m per m.
  const shed = (p: { x: number; y: number }) => 3 + 0.5 * p.y;
  const none = () => null;

  it("raises a verge wall to the sloping roof (a trapezoid)", () => {
    const w = wallUnderRoof({ x: 10, y: 0 }, { x: 10, y: 6 }, { x: -1, y: 0 }, [], shed, 3);
    expect(w).toHaveLength(4);
    expect(area3(w)).toBeCloseTo(6 * (3 + 6) / 2, 3);
  });

  it("raises the high wall to the roof's top edge", () => {
    const w = wallUnderRoof({ x: 10, y: 6 }, { x: 0, y: 6 }, { x: 0, y: -1 }, [], shed, 3);
    expect(area3(w)).toBeCloseTo(10 * 6, 1);
  });

  it("follows a ridge over the wall", () => {
    const gable = (p: { x: number; y: number }) => 3 + 0.8 * (3 - Math.abs(p.y - 3));
    const cut = crossings({ x: 0, y: 0 }, { x: 0, y: 6 }, [[[-1, 3, 5.4], [11, 3, 5.4]]]);
    expect(cut).toEqual([0.5]);
    const w = wallUnderRoof({ x: 0, y: 0 }, { x: 0, y: 6 }, { x: 1, y: 0 }, cut, gable, 3);
    expect(w).toHaveLength(5);
    expect(area3(w)).toBeCloseTo(6 * 3 + 0.5 * 6 * 2.4, 2);
  });

  it("stands to the fallback height where no roof is over it", () => {
    const w = wallUnderRoof({ x: 0, y: 0 }, { x: 0, y: 6 }, { x: 1, y: 0 }, [], none, 2.5);
    expect(w).toEqual([[0, 0, 0], [0, 6, 0], [0, 6, 2.5], [0, 0, 2.5]]);
  });

  it("steps where the roof over the wall ends (a vertical edge, still drawable)", () => {
    // Roof only over the first half of the wall; the rest stands to 2 m.
    const half = (p: { x: number; y: number }) => (p.y < 3 ? 4 : null);
    const w = wallUnderRoof({ x: 0, y: 0 }, { x: 0, y: 6 }, { x: 1, y: 0 }, [0.5], half, 2);
    expect(area3(w)).toBeCloseTo(3 * 4 + 3 * 2, 6);
    const idx = wallTriangles(w)!;
    expect(trianglesArea(w, idx)).toBeCloseTo(18, 6);
  });
});

describe("a closed building in 3D (no gap under the roof, walls meet at the corners)", () => {
  // The core's result for a 12 × 8 m gable roof, 40°, eave 3 m, overhang
  // 0.5 m (tectora calc on the "sedlova-strecha" example): the eave is at the
  // overhang line, so the roof passes over the wall line 0.42 m above it.
  const rise = Math.tan((40 * Math.PI) / 180);
  const ridge = 3 + 4.5 * rise;
  const faces: Poly3[] = [
    [[-0.5, 4, ridge], [12.5, 4, ridge], [12.5, -0.5, 3], [-0.5, -0.5, 3]],
    [[12.5, 4, ridge], [-0.5, 4, ridge], [-0.5, 8.5, 3], [12.5, 8.5, 3]],
  ];
  const gables: Poly3[] = [
    [[12.5, 8.5, 3], [12.5, 4, ridge], [12.5, -0.5, 3]],
    [[-0.5, 8.5, 3], [-0.5, 4, ridge], [-0.5, -0.5, 3]],
  ];
  const result = { parts: [{ geometry: { faces: faces.map((polygon, k) => ({ edge: k * 2, polygon })) } }] } as unknown as CalcResult;
  const roofAt = (p: { x: number; y: number }) => hostHeightAt(result, p);
  const corners = [{ x: 0, y: 0 }, { x: 12, y: 0 }, { x: 12, y: 8 }, { x: 0, y: 8 }];
  const ridgeLine: [Poly3[number], Poly3[number]][] = [[[-0.5, 4, ridge], [12.5, 4, ridge]]];
  // The building height is below the roof at the wall line on purpose.
  const walls = corners.map((a, i) => {
    const b = corners[(i + 1) % 4];
    const inward = inwardNormal(corners, a, b);
    return { wall: wallUnderRoof(a, b, inward, crossings(a, b, ridgeLine), roofAt, 3), inward };
  });

  it("turns the wall normals into the building for either drawing direction", () => {
    expect(inwardNormal(corners, corners[0], corners[1]).y).toBeCloseTo(1, 12);
    // The same building drawn clockwise: its first wall is the one on y = 8.
    const cw = [...corners].reverse();
    const n = inwardNormal(cw, cw[0], cw[1]);
    expect(n.x).toBeCloseTo(0, 12);
    expect(n.y).toBeCloseTo(-1, 12);
  });

  it("carries every wall up to the roof over its line, not to the eave height", () => {
    const atWall = 3 + 0.5 * rise;
    for (const { wall } of walls) {
      for (let k = 0; k <= 10; k++) {
        const t = k / 10;
        const p = { x: wall[0][0] + (wall[1][0] - wall[0][0]) * t, y: wall[0][1] + (wall[1][1] - wall[0][1]) * t };
        const roof = 3 + (4.5 - Math.abs(p.y - 4)) * rise;
        expect(wallTopAt(wall, t)).toBeCloseTo(roof, 2);
      }
    }
    expect(wallTopAt(walls[0].wall, 0.5)).toBeCloseTo(atWall, 2);
    expect(wallTopAt(walls[1].wall, 0.5)).toBeCloseTo(ridge - 0, 0);
  });

  it("gives neighbouring walls the same height at their shared corner", () => {
    walls.forEach(({ wall }, i) => {
      const next = walls[(i + 1) % 4].wall;
      expect(wallTopAt(wall, 1)).toBeCloseTo(wallTopAt(next, 0), 2);
    });
  });

  it("draws every wall as triangles of exactly its area", () => {
    for (const { wall } of walls) {
      const idx = wallTriangles(wall)!;
      expect(trianglesArea(wall, idx)).toBeCloseTo(area3(wall), 6);
    }
  });

  it("leaves out the calculation's gable in the verge plane, which the wall replaces", () => {
    expect(gableShownByWall(gables[0], walls[1].wall, walls[1].inward, 0.5)).toBe(true);
    expect(gableShownByWall(gables[1], walls[3].wall, walls[3].inward, 0.5)).toBe(true);
    // Not the gable of the other end, and not one on an eave wall.
    expect(gableShownByWall(gables[1], walls[1].wall, walls[1].inward, 0.5)).toBe(false);
    expect(gableShownByWall(gables[0], walls[0].wall, walls[0].inward, 0.5)).toBe(false);
  });

  it("keeps a gable that rises above the wall (a dormer front on the wall line)", () => {
    const front: Poly3 = [[4, 0, 3], [6, 0, 3], [6, 0, 5], [5, 0, 5.8], [4, 0, 5]];
    expect(gableShownByWall(front, walls[0].wall, walls[0].inward, 0.5)).toBe(false);
  });
});

describe("manualFacePoly (manual faces in 3D, UX-009)", () => {
  it("lies on a plane of its pitch, so its 3D area is the face's area", () => {
    const pts = [{ x: 16, y: 0 }, { x: 20, y: 0 }, { x: 20, y: 4 }, { x: 16, y: 4 }];
    for (const pitch of [0, 30, 45]) {
      const poly = manualFacePoly(pts, pitch, 3)!;
      expect(area3(poly)).toBeCloseTo(planArea(pts) / Math.cos((pitch * Math.PI) / 180), 6);
      expect(Math.min(...poly.map((p) => p[2]))).toBeCloseTo(3, 9);
    }
  });

  it("refuses a face it cannot draw", () => {
    expect(manualFacePoly([{ x: 0, y: 0 }, { x: 1, y: 0 }], 30, 0)).toBeNull();
    expect(manualFacePoly([{ x: 0, y: 0 }, { x: 1, y: 0 }, { x: 1, y: 1 }], 90, 0)).toBeNull();
  });
});

describe("outgrows (3D refits for a second building, UI-007)", () => {
  const box = (x0: number, x1: number) => ({ min: [x0, 0, -8] as [number, number, number], max: [x1, 7, 0] as [number, number, number] });
  it("is true for a scene reaching outside the framed one, false within it", () => {
    expect(outgrows(null, box(0, 12))).toBe(true);
    expect(outgrows(box(0, 12), box(0, 26))).toBe(true);
    expect(outgrows(box(0, 12), box(0.2, 12.2))).toBe(false);
    expect(outgrows(box(0, 26), box(0, 12))).toBe(false);
  });
});

/** Area of the triangles the 3D view builds, in the wall plane. */
function triangulatedArea(uv: [number, number][]): number {
  const contour = uv.map(([u, v]) => new Vector2(u, v));
  return ShapeUtils.triangulateShape(contour, []).reduce((s, [a, b, c]) => s + Math.abs(ShapeUtils.area([contour[a], contour[b], contour[c]])), 0);
}

describe("gableContour", () => {
  it("lays a plain gable triangle into its wall plane", () => {
    const uv = gableContour([
      [12.5, 8.5, 3],
      [12.5, 4, 6.78],
      [12.5, -0.5, 3],
    ])!;
    expect(uv.map(([u]) => Math.abs(u))).toEqual([0, 4.5, 9]);
    expect(triangulatedArea(uv)).toBeCloseTo(0.5 * 9 * 3.78, 6);
  });

  it("keeps a gable that starts with a vertical edge (cut by a lower roof)", () => {
    // The first roof's gable once a second, lower roof runs under its
    // overhang (from the core's multi-roof join): it starts at a vertical
    // edge, which used to collapse the gable and skew it in 3D.
    const poly: [number, number, number][] = [
      [12.5, 1.2, 3.0],
      [12.5, 1.2, 3.59],
      [12.5, 6.8, 3.59],
      [12.5, 7.0, 3.42],
      [12.5, 7.0, 3.0],
      [12.5, 8.5, 3.0],
      [12.5, 4.0, 6.78],
      [12.5, -0.5, 3.0],
    ];
    const uv = gableContour(poly)!;
    expect(uv).not.toBeNull();
    // Shoelace area of the notched triangle, in the plane (y, z).
    const shoelace = Math.abs(poly.reduce((s, p, i) => {
      const q = poly[(i + 1) % poly.length];
      return s + p[1] * q[2] - q[1] * p[2];
    }, 0)) / 2;
    expect(triangulatedArea(uv)).toBeCloseTo(shoelace, 6);
  });

  it("works for a wall in any direction", () => {
    const s = Math.SQRT1_2;
    const uv = gableContour([
      [0, 0, 0],
      [4 * s, 4 * s, 0],
      [2 * s, 2 * s, 2],
    ])!;
    expect(uv[1][0]).toBeCloseTo(4, 9);
    expect(triangulatedArea(uv)).toBeCloseTo(4, 9);
  });

  it("refuses a polygon with no horizontal extent", () => {
    expect(gableContour([
      [1, 1, 0],
      [1, 1, 2],
      [1, 1, 3],
    ])).toBeNull();
  });
});

describe("dormerFaces (dormers one behind the other, UI-002)", () => {
  // Two dormers 2 m wide on x = 2..4: fronts on y = 0 and y = 1.6, cheeks
  // on the shared lines x = 2 and x = 4.
  const dormer = (y: number) =>
    dormerWallLines({
      edges: [
        { start: { x: 2, y: y + 1 }, direction_rad: -Math.PI / 2 },
        { start: { x: 2, y } },
        { start: { x: 4, y }, direction_rad: Math.PI / 2 },
      ],
      end: { x: 4, y: y + 1 },
    } as never);
  const lines = [dormer(1.6), dormer(0)];
  // Hip dormers: a front face on the front wall, side faces on a cheek only.
  const hip = (y: number): Poly3[] => [
    [[2, y, 4], [4, y, 4], [3, y + 0.5, 4.6]],
    [[2, y, 4], [3, y + 0.5, 4.6], [3, y + 1.2, 4.6], [2, y + 1.2, 4]],
    [[4, y, 4], [4, y + 1.2, 4], [3, y + 1.2, 4.6], [3, y + 0.5, 4.6]],
  ];

  it("gives every face to its own dormer, whichever dormer comes first", () => {
    expect(dormerFaces(lines, [...hip(1.6), ...hip(0)])).toEqual([0, 0, 0, 1, 1, 1]);
    expect(dormerFaces(lines, [...hip(0), ...hip(1.6)])).toEqual([1, 1, 1, 0, 0, 0]);
    expect(dormerFaces([lines[1], lines[0]], [...hip(1.6), ...hip(0)])).toEqual([1, 1, 1, 0, 0, 0]);
  });

  it("gives -1 to a face on no dormer wall line", () => {
    expect(dormerFaces(lines, [[[8, 8, 3], [9, 8, 3], [9, 9, 4]]])).toEqual([-1]);
  });
});
