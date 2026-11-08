import { describe, expect, it } from "vitest";
import { ShapeUtils, Vector2 } from "three";
import { gableContour } from "./scene3d";

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
