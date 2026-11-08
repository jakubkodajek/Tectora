// Plan geometry for the 3D view: roof planes, clipping a roof opening to a
// face, the wall lines of a dormer and the height range of a chimney.
// Display only; quantities come from the core.

import type { Chimney, Dormer, Point2 } from "./types";

export type Poly3 = [number, number, number][];

/** Height of a polygon's plane over a plan point, or null for a vertical polygon (Newell normal). */
export function planeOf(poly: Poly3): ((p: Point2) => number) | null {
  let [nx, ny, nz] = [0, 0, 0];
  for (let i = 0; i < poly.length; i++) {
    const [a, b] = [poly[i], poly[(i + 1) % poly.length]];
    nx += (a[1] - b[1]) * (a[2] + b[2]);
    ny += (a[2] - b[2]) * (a[0] + b[0]);
    nz += (a[0] - b[0]) * (a[1] + b[1]);
  }
  if (Math.abs(nz) < 1e-9 * (Math.abs(nx) + Math.abs(ny) + 1)) return null;
  const o = poly[0];
  return (p) => o[2] - (nx * (p.x - o[0]) + ny * (p.y - o[1])) / nz;
}

/** Unsigned plan area. O(n). */
export function planArea(pts: Point2[]): number {
  let a = 0;
  for (let i = 0; i < pts.length; i++) {
    const [p, q] = [pts[i], pts[(i + 1) % pts.length]];
    a += p.x * q.y - q.x * p.y;
  }
  return Math.abs(a) / 2;
}

/**
 * Part of `subject` inside the convex polygon `clip` (Sutherland–Hodgman;
 * `clip` may run either way). O(n·m).
 */
export function clipConvex(subject: Point2[], clip: Point2[]): Point2[] {
  if (subject.length < 3 || clip.length < 3) return [];
  let s2 = 0;
  for (let i = 0; i < clip.length; i++) {
    const [p, q] = [clip[i], clip[(i + 1) % clip.length]];
    s2 += p.x * q.y - q.x * p.y;
  }
  const sign = s2 >= 0 ? 1 : -1;
  let out = subject;
  for (let i = 0; i < clip.length && out.length; i++) {
    const [a, b] = [clip[i], clip[(i + 1) % clip.length]];
    const side = (p: Point2) => sign * ((b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x));
    const input = out;
    out = [];
    for (let k = 0; k < input.length; k++) {
      const [p, q] = [input[k], input[(k + 1) % input.length]];
      const [sp, sq] = [side(p), side(q)];
      if (sp >= 0) out.push(p);
      if ((sp >= 0) !== (sq >= 0)) {
        const u = sp / (sp - sq);
        out.push({ x: p.x + (q.x - p.x) * u, y: p.y + (q.y - p.y) * u });
      }
    }
  }
  return out;
}

/** A dormer wall line: a front segment, or the line of a cheek through a
 *  front corner. The cheek's direction marker points away from the roof
 *  (the dormer roof lies on the other side), so the line runs both ways. */
export interface WallLine {
  a: Point2;
  dir: Point2;
  /** Segment length; Infinity for a cheek line. */
  len: number;
}

/** The front walls and cheek lines of a dormer's open wall chain. */
export function dormerWallLines(d: Dormer): WallLine[] {
  const pts = [...d.edges.map((e) => e.start), d.end];
  const out: WallLine[] = [];
  d.edges.forEach((e, k) => {
    if (e.direction_rad != null) {
      out.push({ a: e.start, dir: { x: Math.cos(e.direction_rad), y: Math.sin(e.direction_rad) }, len: Infinity });
      return;
    }
    const b = pts[k + 1];
    const len = Math.hypot(b.x - e.start.x, b.y - e.start.y);
    if (len > 1e-6) out.push({ a: e.start, dir: { x: (b.x - e.start.x) / len, y: (b.y - e.start.y) / len }, len });
  });
  return out;
}

// How far a roof edge may lie off a wall line and still hang a wall from it, m.
const ON_LINE = 0.02;

/** True when the plan segment p–q lies on one of the wall lines. */
export function onWallLine(lines: WallLine[], p: [number, number, number], q: [number, number, number]): boolean {
  if (Math.hypot(q[0] - p[0], q[1] - p[1]) < 1e-3) return false;
  return lines.some((l) => {
    const on = (r: [number, number, number]) => {
      const [dx, dy] = [r[0] - l.a.x, r[1] - l.a.y];
      const s = dx * l.dir.x + dy * l.dir.y;
      const along = l.len === Infinity || (s > -ON_LINE && s < l.len + ON_LINE);
      return Math.abs(dx * l.dir.y - dy * l.dir.x) < ON_LINE && along;
    };
    return on(p) && on(q);
  });
}

/**
 * Height range drawn for a chimney: from just under the lowest roof point
 * at its corners to its top. A top at or below the roof (e.g. not given)
 * is shown 1 m above the highest roof point [no source: display default].
 */
export function chimneyTop(c: Chimney, host: (p: Point2) => number | null): { bottom: number; top: number } {
  const zs = c.corners.map(host).filter((z): z is number => z !== null);
  if (!zs.length) return { bottom: 0, top: Math.max(c.height, 1) };
  const [lo, hi] = [Math.min(...zs), Math.max(...zs)];
  return { bottom: Math.max(0, lo - 0.3), top: c.height > hi + 0.05 ? c.height : hi + 1 };
}

/**
 * A vertical polygon (a gable) in its own wall plane: u along the wall, v the
 * height, one pair per corner. The wall direction comes from the two corners
 * farthest apart in plan, not from the first edge: a gable cut by a lower
 * roof starts with a vertical edge (both corners at one plan point), and a
 * direction from it collapsed the whole gable to a line, so the 3D view drew
 * it skewed. Null when the polygon has no horizontal extent.
 */
export function gableContour(poly: Poly3): [number, number][] | null {
  if (poly.length < 3) return null;
  const p0 = poly[0];
  let far = p0;
  let best = 0;
  for (const p of poly) {
    const d = Math.hypot(p[0] - p0[0], p[1] - p0[1]);
    if (d > best) {
      best = d;
      far = p;
    }
  }
  if (best < 1e-6) return null;
  const d = [(far[0] - p0[0]) / best, (far[1] - p0[1]) / best];
  return poly.map((p) => [(p[0] - p0[0]) * d[0] + (p[1] - p0[1]) * d[1], p[2]]);
}
