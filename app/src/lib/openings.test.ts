import { describe, expect, it } from "vitest";
import { openingBox } from "./openings";

// The editor makes chimneys through the core (`chimney_from_rect`); the same
// rounding here builds the input. The core's own tests of `reshape_opening`
// and `chimney_from_rect` (edit/openings.rs) carry the former reshapeOpening
// cases.
function chimneyFromRect(r: { x: number; y: number; width: number; depth: number; rotation: number; height: number }) {
  const a = (r.rotation * Math.PI) / 180;
  const u = { x: Math.cos(a), y: Math.sin(a) };
  const p = (s: number, t: number) => ({ x: +(r.x + u.x * s - u.y * t).toFixed(3), y: +(r.y + u.y * s + u.x * t).toFixed(3) });
  return { corners: [p(0, 0), p(r.width, 0), p(r.width, r.depth), p(0, r.depth)], height: r.height };
}

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
