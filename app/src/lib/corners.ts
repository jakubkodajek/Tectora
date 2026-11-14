// Dragging the corners of a part's outline: a direction marker moves with
// the corner it sits on, and a corner dropped onto its neighbour merges.

import type { EdgeDef, Point2 } from "./types";
import { isMarker } from "./types";

const at = (a: Point2, b: Point2, tol: number) => Math.abs(a.x - b.x) < tol && Math.abs(a.y - b.y) < tol;

/** Two plan points closer than 1 cm (the core's `MIN_WALL_LENGTH`) count as one corner. */
export const samePoint = (a: Point2, b: Point2) => Math.hypot(a.x - b.x, a.y - b.y) < 0.01;

/**
 * A drawn footprint without repeated corners: a corner on the one before it
 * and closing corners on the first one are dropped, so no wall has zero
 * length. O(n).
 */
export function withoutRepeats(pts: Point2[]): Point2[] {
  const out: Point2[] = [];
  for (const p of pts) if (!out.length || !samePoint(out[out.length - 1], p)) out.push(p);
  while (out.length > 1 && samePoint(out[out.length - 1], out[0])) out.pop();
  return out;
}

/** Corner `v` and the direction markers on the same point next to it. O(n). */
export function cornerGroup(es: EdgeDef[], v: number): number[] {
  const n = es.length;
  if (!es[v]) return [];
  const p = es[v].start;
  const out = [v];
  // Markers just before v on the same point.
  for (let k = 1; k < n; k++) {
    const j = (v - k + n) % n;
    if (!isMarker(es[j]) || !at(es[j].start, p, 1e-6)) break;
    out.push(j);
  }
  // v is a marker: the corners after it on the same point (markers, then the wall).
  for (let k = 1; k < n; k++) {
    const prev = (v + k - 1) % n;
    const j = (v + k) % n;
    if (!isMarker(es[prev]) || !at(es[j].start, p, 1e-6)) break;
    out.push(j);
  }
  return [...new Set(out)];
}

/** Moves corner `v` (with its markers) to (x, y). */
export function moveCorner(es: EdgeDef[], v: number, x: number, y: number) {
  for (const j of cornerGroup(es, v)) es[j].start = { x, y };
}

/**
 * After a drag: when corner `v` lies on the next or previous wall corner
 * (within 5 mm) it is removed, as long as three walls remain. Returns true
 * when it was merged.
 */
export function mergeDroppedCorner(es: EdgeDef[], v: number): boolean {
  const n = es.length;
  if (!es[v] || isMarker(es[v]) || es.filter((e) => !isMarker(e)).length <= 3) return false;
  const p = es[v].start;
  const wallAt = (dir: 1 | -1) => {
    for (let k = 1; k < n; k++) {
      const j = (v + dir * k + n * k) % n;
      if (!isMarker(es[j])) return j;
    }
    return -1;
  };
  const hit = [wallAt(1), wallAt(-1)].some((j) => j >= 0 && at(es[j].start, p, 0.005));
  if (!hit) return false;
  // Markers on the removed corner go with it.
  const drop = cornerGroup(es, v).sort((a, b) => b - a);
  for (const j of drop) es.splice(j, 1);
  return true;
}

/**
 * The point on wall a–b nearest to `p`, its distance from `a` rounded to
 * `step` and kept at least 1 cm (the core's shortest wall) from both ends;
 * null on a wall too short for a corner. Coordinates to the millimetre.
 */
export function onWall(a: Point2, b: Point2, p: Point2, step: number): Point2 | null {
  const len = Math.hypot(b.x - a.x, b.y - a.y);
  const min = 0.01;
  if (len < 2 * min) return null;
  const [ux, uy] = [(b.x - a.x) / len, (b.y - a.y) / len];
  let s = (p.x - a.x) * ux + (p.y - a.y) * uy;
  s = step > 0 ? Math.round(s / step) * step : s;
  s = Math.min(len - min, Math.max(min, s));
  return { x: +(a.x + ux * s).toFixed(3), y: +(a.y + uy * s).toFixed(3) };
}
