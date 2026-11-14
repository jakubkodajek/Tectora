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

/** True when plan point r lies on wall line l (within ON_LINE). */
function onLine(l: WallLine, r: readonly number[]): boolean {
  const [dx, dy] = [r[0] - l.a.x, r[1] - l.a.y];
  const s = dx * l.dir.x + dy * l.dir.y;
  const along = l.len === Infinity || (s > -ON_LINE && s < l.len + ON_LINE);
  return Math.abs(dx * l.dir.y - dy * l.dir.x) < ON_LINE && along;
}

/** True when the plan segment p–q lies on one of the wall lines. */
export function onWallLine(lines: WallLine[], p: [number, number, number], q: [number, number, number]): boolean {
  if (Math.hypot(q[0] - p[0], q[1] - p[1]) < 1e-3) return false;
  return lines.some((l) => onLine(l, p) && onLine(l, q));
}

/** Plan extent (x, y) of a dormer's roof faces; null without faces. */
export type PlanBox = { minX: number; minY: number; maxX: number; maxY: number };

/** The plan box around a set of 3D polygons, or null when there are none. */
export function planBox(polys: Poly3[]): PlanBox | null {
  const pts = polys.flat();
  if (!pts.length) return null;
  return {
    minX: Math.min(...pts.map((p) => p[0])),
    minY: Math.min(...pts.map((p) => p[1])),
    maxX: Math.max(...pts.map((p) => p[0])),
    maxY: Math.max(...pts.map((p) => p[1])),
  };
}

/**
 * True when a wall polygon of the calculation (a gable) stands on a wall
 * line of a dormer and within that dormer's roof: the scene already hangs
 * that dormer's walls from its roof edges down to the main roof, so drawing
 * the gable as well puts two surfaces in one plane, which flicker
 * (z-fighting) as the camera moves. A main building's gable in the same
 * plane as a dormer cheek reaches beyond the dormer and is kept.
 */
export function coveredByDormer(poly: Poly3, dormers: { lines: WallLine[]; box: PlanBox | null }[]): boolean {
  if (poly.length < 3) return false;
  return dormers.some(({ lines, box }) => {
    if (!box) return false;
    const inBox = poly.every((p) => p[0] > box.minX - ON_LINE && p[0] < box.maxX + ON_LINE && p[1] > box.minY - ON_LINE && p[1] < box.maxY + ON_LINE);
    return inBox && lines.some((l) => poly.every((p) => onLine(l, p)));
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

// How far inside the building the roof is read for a wall's top, m: a wall
// without overhang lies on the edge of its roof face, where "over the wall"
// is not decided.
const WALL_INSET = 1e-3;

/**
 * A wall from the ground up to the underside of the roof over its line, so
 * the building is closed from every side: the bottom edge a–b at z = 0, the
 * top following the roof between the points `cuts` (plan parameters along
 * a–b where the roof's surface bends or ends, e.g. a ridge over a gable
 * wall or the valley of a joined roof). The eave height of a wall is
 * measured at its overhang line, so the roof passes over the wall line
 * higher than the eave: a wall drawn to the eave height leaves a gap under
 * the roof. `inward` is the unit normal of the wall pointing into the
 * building; `fallback` is the top where no roof is over the wall (no
 * calculation, or a building left out of it). Each stretch between two cuts
 * is read inside the stretch, so a step in the roof gives a vertical edge
 * [no source: display only].
 */
export function wallUnderRoof(a: Point2, b: Point2, inward: Point2, cuts: number[], roofAt: (p: Point2) => number | null, fallback: number): Poly3 {
  const ts = [0, ...cuts.filter((t) => t > 1e-6 && t < 1 - 1e-6), 1];
  const at = (t: number): Point2 => ({ x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t });
  const read = (t: number) => {
    const p = at(t);
    const z = roofAt({ x: p.x + inward.x * WALL_INSET, y: p.y + inward.y * WALL_INSET });
    return z !== null && Number.isFinite(z) ? z : null;
  };
  const floor = Math.max(0.01, fallback);
  const top: Poly3 = [];
  const push = (t: number, z: number) => {
    const p = at(t);
    const last = top[top.length - 1];
    const q: Poly3[number] = [p.x, p.y, Math.max(0.01, z)];
    if (last && Math.hypot(last[0] - q[0], last[1] - q[1]) < 1e-9 && Math.abs(last[2] - q[2]) < 1e-4) return;
    top.push(q);
  };
  for (let i = 0; i + 1 < ts.length; i++) {
    const [t0, t1] = [ts[i], ts[i + 1]];
    const eps = (t1 - t0) / 8;
    const [k0, k1] = [t0 + eps, t1 - eps];
    const [z0, z1] = [read(k0), read(k1)];
    if (z0 === null || z1 === null) {
      push(t0, z0 ?? z1 ?? floor);
      push(t1, z1 ?? z0 ?? floor);
      continue;
    }
    // One plane over the stretch: carry the two readings out to its ends.
    const slope = (z1 - z0) / (k1 - k0);
    push(t0, z0 - slope * (k0 - t0));
    push(t1, z1 + slope * (t1 - k1));
  }
  return [[a.x, a.y, 0], [b.x, b.y, 0], ...top.reverse()];
}

/** Height of the top of a wall from `wallUnderRoof` at plan parameter t (0..1, clamped). */
export function wallTopAt(wall: Poly3, t: number): number {
  const [a, b] = [wall[0], wall[1]];
  const len2 = (b[0] - a[0]) ** 2 + (b[1] - a[1]) ** 2 || 1;
  const param = (p: Poly3[number]) => ((p[0] - a[0]) * (b[0] - a[0]) + (p[1] - a[1]) * (b[1] - a[1])) / len2;
  // The top runs from b back to a.
  const top = wall.slice(2).map((p) => ({ t: param(p), z: p[2] })).reverse();
  const k = Math.min(1, Math.max(0, t));
  let best = top[0].z;
  for (let i = 0; i + 1 < top.length; i++) {
    const [p, q] = [top[i], top[i + 1]];
    if (k < p.t - 1e-9 || k > q.t + 1e-9) continue;
    const z = q.t - p.t < 1e-9 ? Math.max(p.z, q.z) : p.z + ((q.z - p.z) * (k - p.t)) / (q.t - p.t);
    best = i === 0 ? z : Math.max(best, z);
    if (k < q.t - 1e-9) return z;
  }
  return best;
}

/**
 * True when a gable polygon of the calculation is already shown by `wall`
 * (from `wallUnderRoof`): it stands in the wall's plane or in the plane of
 * the wall's verge, at most `overhang` m outside it, and nowhere rises
 * above the wall's top. The calculation puts a gable in the plane of the
 * overhang line, so beside a wall drawn up to the roof it would hang in
 * front of the building as a loose sheet, and without overhang it would
 * lie in the wall's own plane and flicker. A gable that rises above the
 * wall (a dormer front standing on the wall line) is kept.
 */
export function gableShownByWall(gable: Poly3, wall: Poly3, inward: Point2, overhang: number): boolean {
  if (gable.length < 3) return false;
  const [a, b] = [wall[0], wall[1]];
  const len = Math.hypot(b[0] - a[0], b[1] - a[1]);
  if (len < 1e-6) return false;
  const out = gable.map((p) => -((p[0] - a[0]) * inward.x + (p[1] - a[1]) * inward.y));
  const [lo, hi] = [Math.min(...out), Math.max(...out)];
  if (hi - lo > ON_LINE || lo < -ON_LINE || hi > Math.max(0, overhang) + ON_LINE) return false;
  return gable.every((p) => {
    const t = ((p[0] - a[0]) * (b[0] - a[0]) + (p[1] - a[1]) * (b[1] - a[1])) / (len * len);
    // Past the wall's ends lies only the verge over the neighbours' overhang.
    if (t < -2 / len || t > 1 + 2 / len) return false;
    return p[2] <= wallTopAt(wall, t) + ON_LINE;
  });
}

/** Unit normal of side a–b pointing into the closed plan polygon `pts`. */
export function inwardNormal(pts: Point2[], a: Point2, b: Point2): Point2 {
  let s2 = 0;
  for (let i = 0; i < pts.length; i++) {
    const [p, q] = [pts[i], pts[(i + 1) % pts.length]];
    s2 += p.x * q.y - q.x * p.y;
  }
  const len = Math.hypot(b.x - a.x, b.y - a.y) || 1;
  const sign = s2 >= 0 ? 1 : -1;
  return { x: (-(b.y - a.y) / len) * sign, y: ((b.x - a.x) / len) * sign };
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
