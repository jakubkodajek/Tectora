// DACAD's Runddach helper (Parameter > Neigungen berechnen > Runddach): on
// two parallel walls with the same eave height it sets pitches and Knicke so
// that an inscribed polygon approximates a circular arc. The arc ends at the
// overhang tips; by default it is a semicircle (Tonnendach). Source: DACAD.HLP
// topic "Runddach" and the RNDROOF dialog bitmap.

import type { EdgeDef, PitchBreak, Point2 } from "./types";
import { isMarker } from "./types";

export type ArcMode = "half" | "radius" | "ridge" | "length";

export interface ArcInput {
  /** Distance between the two wall lines, m. */
  span: number;
  overhang: number;
  eave_height: number;
  /** Segments of the polygon on each side of the ridge. */
  segments: number;
  mode: ArcMode;
  /** The radius, ridge height or developed length, by mode. */
  value: number;
}

export interface Arc {
  radius: number;
  ridge: number;
  /** Developed length of the whole arc, eave tip to eave tip. */
  length: number;
  pitch_deg: number;
  breaks: PitchBreak[];
}

/** Radius of the semicircle over the span; any arc needs at least this. */
export const minRadius = (span: number, overhang: number) => span / 2 + overhang;

/** The arc for the input, or null when the value is out of range. */
export function arcFor(a: ArcInput): Arc | null {
  const r0 = minRadius(a.span, a.overhang);
  if (!(r0 > 0) || a.segments < 1) return null;
  let r = r0;
  if (a.mode === "radius") {
    if (a.value < r0 - 1e-9) return null;
    r = a.value;
  } else if (a.mode === "ridge") {
    // Rise above the eave tips: r - sqrt(r² - r0²) = rise.
    const rise = a.value - a.eave_height;
    if (!(rise > 0) || rise > r0 + 1e-9) return null;
    r = (rise * rise + r0 * r0) / (2 * rise);
  } else if (a.mode === "length") {
    // Developed length 2·r·asin(r0/r) falls from π·r0 towards 2·r0 as r grows.
    if (a.value > Math.PI * r0 + 1e-9 || a.value <= 2 * r0) return null;
    let lo = r0;
    let hi = r0 * 1e4;
    for (let k = 0; k < 200; k++) {
      const mid = (lo + hi) / 2;
      if (2 * mid * Math.asin(r0 / mid) > a.value) lo = mid;
      else hi = mid;
    }
    r = (lo + hi) / 2;
  }
  const half = Math.asin(Math.min(1, r0 / r));
  const centre = a.eave_height - Math.sqrt(Math.max(0, r * r - r0 * r0));
  // Vertices at equal angles; a chord's pitch is the angle of the radius to
  // its middle, measured from the vertical.
  const at = (k: number) => half * (1 - k / a.segments);
  const pitch = (k: number) => +(((at(k) + at(k + 1)) / 2) * (180 / Math.PI)).toFixed(3);
  const breaks: PitchBreak[] = [];
  for (let k = 1; k < a.segments; k++) {
    breaks.push({ height: +(centre + r * Math.cos(at(k))).toFixed(3), pitch_deg: pitch(k) });
  }
  return { radius: r, ridge: centre + r, length: 2 * r * half, pitch_deg: pitch(0), breaks };
}

/** Index of the wall facing edge `i` across the part (anti-parallel, overlapping), or null. */
export function oppositeWall(edges: EdgeDef[], i: number): number | null {
  const n = edges.length;
  const seg = (k: number) => ({ a: edges[k].start, b: edges[(k + 1) % n].start });
  const { a, b } = seg(i);
  const len = Math.hypot(b.x - a.x, b.y - a.y);
  if (len < 1e-6) return null;
  const u = { x: (b.x - a.x) / len, y: (b.y - a.y) / len };
  // The roof rises to the left of a wall (counter-clockwise outline).
  const inward = { x: -u.y, y: u.x };
  let best: { k: number; d: number } | null = null;
  for (let k = 0; k < n; k++) {
    if (k === i || isMarker(edges[k])) continue;
    const s = seg(k);
    const l = Math.hypot(s.b.x - s.a.x, s.b.y - s.a.y);
    if (l < 1e-6) continue;
    const v = { x: (s.b.x - s.a.x) / l, y: (s.b.y - s.a.y) / l };
    if (u.x * v.x + u.y * v.y > -1 + 1e-6) continue;
    const d = dot(sub(s.a, a), inward);
    if (d <= 1e-6) continue;
    // Their extents along the wall must overlap.
    const t0 = dot(sub(s.a, a), u);
    const t1 = dot(sub(s.b, a), u);
    if (Math.max(t0, t1) <= 1e-6 || Math.min(t0, t1) >= len - 1e-6) continue;
    if (!best || d < best.d) best = { k, d };
  }
  return best?.k ?? null;
}

/** Distance between edge `i`'s line and edge `j`'s start, across the part. */
export function spanBetween(edges: EdgeDef[], i: number, j: number): number {
  const a = edges[i].start;
  const b = edges[(i + 1) % edges.length].start;
  const l = Math.hypot(b.x - a.x, b.y - a.y);
  return Math.abs(dot(sub(edges[j].start, a), { x: -(b.y - a.y) / l, y: (b.x - a.x) / l }));
}


const sub = (p: Point2, q: Point2) => ({ x: p.x - q.x, y: p.y - q.y });
const dot = (p: Point2, q: Point2) => p.x * q.x + p.y * q.y;
