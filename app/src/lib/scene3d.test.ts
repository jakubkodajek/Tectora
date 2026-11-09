import { describe, expect, it } from "vitest";
import { ShapeUtils, Vector2 } from "three";
import { crossings, gableContour, manualFacePoly, outgrows, planArea, wallTriangles, wallUpToRoof, type Poly3 } from "./scene3d";

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

describe("wallUpToRoof (walls without a gable from the core, UI-010)", () => {
  // A shed roof over 0..10 × 0..6: eave on y = 0 at 3 m, rising 0.5 m per m.
  const shed = (p: { x: number; y: number }) => 3 + 0.5 * p.y;

  it("raises a verge wall to the sloping roof (a trapezoid)", () => {
    const w = wallUpToRoof({ x: 10, y: 0 }, { x: 10, y: 6 }, 0, [], shed)!;
    expect(w).toHaveLength(4);
    expect(area3(w)).toBeCloseTo(6 * (3 + 6) / 2, 3);
  });

  it("raises the high wall to the roof's top edge", () => {
    const w = wallUpToRoof({ x: 10, y: 6 }, { x: 0, y: 6 }, 0, [], shed)!;
    expect(area3(w)).toBeCloseTo(10 * 6, 3);
  });

  it("follows a ridge over the wall and gives nothing where no roof is over it", () => {
    const gable = (p: { x: number; y: number }) => 3 + 0.8 * (3 - Math.abs(p.y - 3));
    const cut = crossings({ x: 0, y: 0 }, { x: 0, y: 6 }, [[[-1, 3, 5.4], [11, 3, 5.4]]]);
    expect(cut).toEqual([0.5]);
    const w = wallUpToRoof({ x: 0, y: 0 }, { x: 0, y: 6 }, 0, cut, gable)!;
    expect(w).toHaveLength(5);
    expect(area3(w)).toBeCloseTo(6 * 3 + 0.5 * 6 * 2.4, 2);
    expect(wallUpToRoof({ x: 0, y: 0 }, { x: 0, y: 6 }, 0, [], () => null)).toBeNull();
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
