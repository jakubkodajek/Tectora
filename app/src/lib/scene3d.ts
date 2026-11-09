// Plan geometry for the 3D view: roof planes, clipping a roof opening to a
// face, the wall lines of a dormer and the height range of a chimney.
// Display only; quantities come from the core.

import { ShapeUtils, Vector2 } from "three";
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

/**
 * The triangles (indices into `poly`) the 3D view draws for a vertical
 * polygon, triangulated in its own wall plane (see `gableContour`). This is
 * the whole step Roof3D runs for a gable, so the test covers its call site.
 * Null when the polygon cannot be drawn.
 */
export function wallTriangles(poly: Poly3): number[] | null {
  const uv = gableContour(poly);
  if (!uv) return null;
  const tris = ShapeUtils.triangulateShape(uv.map(([u, v]) => new Vector2(u, v)), []);
  return tris.length ? tris.flat() : null;
}

/** Plan parameters (0..1) where the segment a–b crosses the plan segments `lines`, sorted. */
export function crossings(a: Point2, b: Point2, lines: [Poly3[number], Poly3[number]][]): number[] {
  const out: number[] = [];
  const [dx, dy] = [b.x - a.x, b.y - a.y];
  for (const [p, q] of lines) {
    const [ex, ey] = [q[0] - p[0], q[1] - p[1]];
    const den = dx * ey - dy * ex;
    if (Math.abs(den) < 1e-12) continue;
    const t = ((p[0] - a.x) * ey - (p[1] - a.y) * ex) / den;
    const u = ((p[0] - a.x) * dy - (p[1] - a.y) * dx) / den;
    if (t > 1e-6 && t < 1 - 1e-6 && u >= -1e-6 && u <= 1 + 1e-6) out.push(t);
  }
  return out.sort((x, y) => x - y).filter((t, i, ts) => i === 0 || t - ts[i - 1] > 1e-6);
}

/**
 * A wall from the ground up to the roof over its line: the bottom edge a–b
 * at `z0`, the top following the roof height at both ends and at every
 * point `cuts` (plan parameters along a–b where the roof's surface bends,
 * e.g. a ridge over a gable wall). Used where the calculation returns no
 * gable polygon for a gable wall (a shed roof's verges and its high wall),
 * so the 3D view shows a closed building [no source: display only].
 * Null when the roof is not over the wall or not above `z0`.
 */
export function wallUpToRoof(a: Point2, b: Point2, z0: number, cuts: number[], roofAt: (p: Point2) => number | null): Poly3 | null {
  const ts = [0, ...cuts.filter((t) => t > 0 && t < 1), 1];
  const top: Poly3 = [];
  for (const t of ts) {
    // Just inside the ends, so the corner itself (shared with the next
    // wall's verge or an eave) does not decide.
    const k = Math.min(1 - 1e-4, Math.max(1e-4, t));
    const p = { x: a.x + (b.x - a.x) * k, y: a.y + (b.y - a.y) * k };
    const z = roofAt(p);
    if (z === null) return null;
    top.push([a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t, z]);
  }
  if (top.every((p) => p[2] <= z0 + 1e-3)) return null;
  return [[a.x, a.y, z0], [b.x, b.y, z0], ...top.reverse().map((p): Poly3[number] => [p[0], p[1], Math.max(p[2], z0)])];
}

/**
 * A hand-drawn roof face (manual face) for the 3D view: the plan polygon
 * lifted onto a plane of its pitch, with its first side as the eave at
 * `base` m, rising towards the polygon. The file gives only outline and
 * pitch, so the slope direction is a display choice [no source: display
 * only]; the 3D area equals the face's area (plan area ÷ cos pitch).
 */
export function manualFacePoly(points: Point2[], pitchDeg: number, base: number): Poly3 | null {
  if (points.length < 3 || !(pitchDeg >= 0 && pitchDeg < 90)) return null;
  const [a, b] = points;
  const len = Math.hypot(b.x - a.x, b.y - a.y);
  if (len < 1e-6) return null;
  // Normal of the first side, turned to the side the polygon lies on.
  let n = { x: -(b.y - a.y) / len, y: (b.x - a.x) / len };
  const c = points.reduce((s, p) => ({ x: s.x + p.x / points.length, y: s.y + p.y / points.length }), { x: 0, y: 0 });
  if ((c.x - a.x) * n.x + (c.y - a.y) * n.y < 0) n = { x: -n.x, y: -n.y };
  const k = Math.tan((pitchDeg * Math.PI) / 180);
  return points.map((p) => [p.x, p.y, base + k * ((p.x - a.x) * n.x + (p.y - a.y) * n.y)]);
}

/** An axis-aligned 3D box, as [min, max] corners. */
export type Extent = { min: [number, number, number]; max: [number, number, number] };

/**
 * True when the scene's extent `now` reaches outside the one the camera was
 * framed for (with 5 % slack): a second building, a copy or a larger roof,
 * which the first framing would cut off (UI-007). Shrinking never reframes,
 * so the user's own view is kept while editing within it.
 */
export function outgrows(framed: Extent | null, now: Extent): boolean {
  if (!framed) return true;
  for (let i = 0; i < 3; i++) {
    const slack = (framed.max[i] - framed.min[i]) * 0.05 + 0.01;
    if (now.min[i] < framed.min[i] - slack || now.max[i] > framed.max[i] + slack) return true;
  }
  return false;
}
