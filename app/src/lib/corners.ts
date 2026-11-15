// Dragging the corners of a part's outline: the plan draws a direction marker
// moving with the corner it sits on while the pointer moves (presentation,
// ADR-0005). What is saved on release (a corner dropped onto its neighbour
// merges, a drawn footprint loses repeated corners) is the core's
// `edit::corners`, through `api.mergeDroppedCorner` / `api.withoutRepeats`.

import type { EdgeDef, Point2 } from "./types";
import { isMarker } from "./types";

const at = (a: Point2, b: Point2, tol: number) => Math.abs(a.x - b.x) < tol && Math.abs(a.y - b.y) < tol;

/** Two plan points closer than 1 cm (the core's `MIN_WALL_LENGTH`) count as one corner. */
export const samePoint = (a: Point2, b: Point2) => Math.hypot(a.x - b.x, a.y - b.y) < 0.01;

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
 * The point on wall a–b nearest to `p`, its distance from `a` rounded to
 * `step` and kept a step, at least 2 cm, from both ends (the core's
 * shortest wall is 1 cm);
 * null on a wall too short for a corner. Coordinates to the millimetre.
 */
export function onWall(a: Point2, b: Point2, p: Point2, step: number): Point2 | null {
  const len = Math.hypot(b.x - a.x, b.y - a.y);
  // Two grid steps (at least 2 cm) from either end: 1 cm itself rounds to
  // 0.0099… and counts as no wall (review: verifier of ERR-002).
  const min = Math.max(step, 0.02);
  if (len < 2 * min) return null;
  const [ux, uy] = [(b.x - a.x) / len, (b.y - a.y) / len];
  let s = (p.x - a.x) * ux + (p.y - a.y) * uy;
  s = step > 0 ? Math.round(s / step) * step : s;
  s = Math.min(len - min, Math.max(min, s));
  return { x: +(a.x + ux * s).toFixed(3), y: +(a.y + uy * s).toFixed(3) };
}
