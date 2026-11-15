// Footprint entry by typed values (draw bar): the next corner from a wall
// length and an angle, and the walls of a footprint being drawn with their
// lengths for the plan. Input handling only; the roof comes from the core.

import type { Point2 } from "./types";

/** A wall as the step from its start corner to its end corner, m. */
export interface Step {
  dx: number;
  dy: number;
}

/** Largest angle the draw bar's angle field takes, degrees (one full turn either way). */
export const MAX_ANGLE = 360;

/** Rounds to mm without a negative zero. */
const mm = (v: number) => {
  const r = +v.toFixed(3);
  return r === 0 ? 0 : r;
};

/** Direction of a step in degrees, 0° = to the right (+x), 90° = up (+y), anticlockwise, in [0, 360). */
export function stepAngle(s: Step): number {
  const a = (Math.atan2(s.dy, s.dx) * 180) / Math.PI;
  return mm(((a % 360) + 360) % 360);
}

/**
 * The step of a wall `length` m long in direction `angleDeg` (0° = right,
 * 90° = up, anticlockwise). With `prev` (the previous wall), the angle is a
 * turn from that wall's direction: positive to the left, negative to the
 * right. Null for a length or angle the field would refuse.
 */
export function angledStep(length: number, angleDeg: number, prev: Step | null = null): Step | null {
  if (!Number.isFinite(length) || !Number.isFinite(angleDeg) || length <= 0 || Math.abs(angleDeg) > MAX_ANGLE) return null;
  const base = prev && (prev.dx !== 0 || prev.dy !== 0) ? stepAngle(prev) : 0;
  const r = ((base + angleDeg) * Math.PI) / 180;
  return { dx: mm(Math.cos(r) * length), dy: mm(Math.sin(r) * length) };
}

/** The last wall of a footprint being drawn, or null before its second corner. */
export function lastStep(points: Point2[] | null | undefined): Step | null {
  if (!points || points.length < 2) return null;
  const a = points[points.length - 2];
  const b = points[points.length - 1];
  return { dx: b.x - a.x, dy: b.y - a.y };
}

/** The walls drawn so far (corner to corner) with their lengths; walls under 1 cm are left out. */
export function draftWalls(points: Point2[] | null | undefined): { a: Point2; b: Point2; length: number }[] {
  const out: { a: Point2; b: Point2; length: number }[] = [];
  if (!points) return out;
  for (let i = 1; i < points.length; i++) {
    const a = points[i - 1];
    const b = points[i];
    const length = Math.hypot(b.x - a.x, b.y - a.y);
    if (length > 0.01) out.push({ a, b, length });
  }
  return out;
}
