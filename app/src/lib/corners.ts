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

/**
 * The direction a new direction change at the start of wall `i` gets: the
 * bisector of the previous wall's direction and this wall's, in radians from
 * 0 to 2 pi. Port of the core's `edit::corners::default_direction` (the MCP
 * tool uses that one); our own choice, not DACAD's (HUNTC-008). The old
 * default, this wall's direction + 90 degrees, never calculated at a
 * right-angled corner.
 */
export function defaultDirection(es: EdgeDef[], i: number): number {
  const n = es.length;
  const TAU = 2 * Math.PI;
  const wrap = (a: number) => ((a % TAU) + TAU) % TAU;
  const e = es[i];
  const next = es[(i + 1) % n].start;
  const dThis = Math.atan2(next.y - e.start.y, next.x - e.start.x);
  let prev: EdgeDef | undefined;
  for (let k = 1; k < n; k++) {
    const c = es[(i + n - k) % n];
    if (!isMarker(c) && !at(c.start, e.start, 1e-9)) {
      prev = c;
      break;
    }
  }
  if (!prev) return wrap(dThis + Math.PI / 4);
  const dPrev = Math.atan2(e.start.y - prev.start.y, e.start.x - prev.start.x);
  const x = Math.cos(dPrev) + Math.cos(dThis);
  const y = Math.sin(dPrev) + Math.sin(dThis);
  const straight = Math.hypot(x, y) < 1e-6 || (Math.abs(Math.sin(dPrev - dThis)) < 1e-9 && x > 0);
  return wrap(straight ? dThis + Math.PI / 4 : Math.atan2(y, x));
}
