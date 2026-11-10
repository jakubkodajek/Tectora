import { describe, expect, it } from "vitest";
import { chimneyFromRect, openingBox, reshapeOpening } from "./openings";

describe("openingBox", () => {
  it("reads a chimney placed along a wall at any angle", () => {
    // Corners rounded to mm used to fail the old rectangle test at most
    // angles, which left the chimney without size fields.
    for (let deg = 0; deg < 360; deg++) {
      const c = chimneyFromRect({ x: 3.217, y: 1.433, width: 0.62, depth: 0.45, rotation: deg, height: 8 });
      const b = openingBox(c.corners)!;
      expect(b, `${deg}`).not.toBeNull();
      expect(Math.abs(b.width - 0.62)).toBeLessThan(2e-3);
      expect(Math.abs(b.depth - 0.45)).toBeLessThan(2e-3);
      const r = (((b.rotation - deg) % 360) + 360) % 360;
      expect(r < 0.2 || r > 359.8, `${deg}: ${b.rotation}`).toBe(true);
    }
  });

  it("gives no box for a polygon without area", () => {
    expect(openingBox([{ x: 0, y: 0 }, { x: 1, y: 0 }, { x: 2, y: 0 }])).toBeNull();
  });
});

describe("reshapeOpening", () => {
  it("resizes a clockwise (mirrored) rectangle and keeps its corner order", () => {
    const cw = [{ x: 0, y: 0 }, { x: 0, y: 0.5 }, { x: 0.8, y: 0.5 }, { x: 0.8, y: 0 }];
    const b = openingBox(cw)!;
    expect([b.width, b.depth]).toEqual([0.5, 0.8]);
    expect(reshapeOpening(cw, { ...b, width: 1 })).toEqual([{ x: 0, y: 0 }, { x: 0, y: 1 }, { x: 0.8, y: 1 }, { x: 0.8, y: 0 }]);
  });

  it("stretches any polygon in its box, as the core does", () => {
    const hex = [{ x: 0, y: 0 }, { x: 1, y: 0 }, { x: 1.5, y: 0.5 }, { x: 1, y: 1 }, { x: 0, y: 1 }, { x: -0.5, y: 0.5 }];
    const b = openingBox(hex)!;
    expect(b).toEqual({ x: -0.5, y: 0, width: 2, depth: 1, rotation: 0 });
    expect(reshapeOpening(hex, { ...b, depth: 2, x: 0.5 })).toEqual([
      { x: 1, y: 0 }, { x: 2, y: 0 }, { x: 2.5, y: 1 }, { x: 2, y: 2 }, { x: 1, y: 2 }, { x: 0.5, y: 1 },
    ]);
  });

  it("keeps the angle when only the width changes and refuses bad sizes", () => {
    const c = chimneyFromRect({ x: 4, y: 1, width: 0.6, depth: 0.6, rotation: 20, height: 6 }).corners;
    const b = openingBox(c)!;
    expect(reshapeOpening(c, b)).toEqual(c);
    const wide = reshapeOpening(c, { ...b, width: 1.2 })!;
    const nb = openingBox(wide)!;
    expect(Math.abs(nb.rotation - b.rotation)).toBeLessThan(0.1);
    expect(Math.abs(nb.width - 1.2)).toBeLessThan(2e-3);
    expect(wide[0]).toEqual(c[0]);
    for (const w of [0, -1, NaN, 1e6]) expect(reshapeOpening(c, { ...b, width: w })).toBeNull();
  });
});
