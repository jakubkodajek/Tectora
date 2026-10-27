// DACAD's Runddach helper (Parameter > Neigungen berechnen > Runddach): on
// two parallel walls with the same eave height it sets pitches and Knicke so
// that an inscribed polygon approximates a circular arc ending at the overhang
// tips (DACAD.HLP topic "Runddach", RNDROOF dialog bitmap). The numbers follow
// DACAD runs RD1-RD4 (/mnt/project-files/dacad-round/): the default arc spans
// 45° on each side, a side gets ceil(arc / 11.25°) segments, and segment k has
// the pitch (k + ½)·arc / n.

import type { EdgeDef, PitchBreak, Point2 } from "./types";
import { isMarker } from "./types";

export type ArcMode = "default" | "radius" | "ridge" | "length";

/** DACAD's default arc on each side of the ridge, degrees. */
const DEFAULT_ARC = 45;
/** DACAD's largest segment angle, degrees. */
const SEGMENT_ARC = 11.25;

export interface ArcInput {
  /** Distance between the two wall lines, m. */
  span: number;
  overhang: number;
  eave_height: number;
  /** Segments of the polygon on each side of the ridge; 0 = DACAD's count. */
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
  segments: number;
  pitch_deg: number;
  breaks: PitchBreak[];
}

/** Half chord of the arc (half the span plus the overhang); the smallest radius. */
export const minRadius = (span: number, overhang: number) => span / 2 + overhang;

/** Radius of DACAD's default arc. */
export const defaultRadius = (span: number, overhang: number) =>
  minRadius(span, overhang) / Math.sin((DEFAULT_ARC * Math.PI) / 180);

/** DACAD's segment count for an arc of `deg` degrees on one side. */
export const segmentsFor = (deg: number) => Math.max(1, Math.ceil(deg / SEGMENT_ARC - 1e-9));

/** The arc for the input, or null when the value is out of range. */
export function arcFor(a: ArcInput): Arc | null {
  const c = minRadius(a.span, a.overhang);
  if (!(c > 0) || a.segments < 0) return null;
  let r = defaultRadius(a.span, a.overhang);
  if (a.mode === "radius") {
    if (a.value < c - 1e-9) return null;
    r = a.value;
  } else if (a.mode === "ridge") {
    // Rise above the eave tips h: r = (c² + h²) / 2h.
    const rise = a.value - a.eave_height;
    if (!(rise > 0) || rise > c + 1e-9) return null;
    r = (rise * rise + c * c) / (2 * rise);
  } else if (a.mode === "length") {
    // Developed length 2·r·asin(c/r) falls from π·c towards 2·c as r grows.
    if (a.value > Math.PI * c + 1e-9 || a.value <= 2 * c) return null;
    let lo = c;
    let hi = c * 1e4;
    for (let k = 0; k < 200; k++) {
      const mid = (lo + hi) / 2;
      if (2 * mid * Math.asin(c / mid) > a.value) lo = mid;
      else hi = mid;
    }
    r = (lo + hi) / 2;
  }
  const half = Math.asin(Math.min(1, c / r));
  const n = a.segments || segmentsFor((half * 180) / Math.PI);
  const centre = a.eave_height - Math.sqrt(Math.max(0, r * r - c * c));
  // Vertices at equal angles from the vertical; a chord's pitch is the angle
  // of the radius to its middle.
  const at = (k: number) => half * (1 - k / n);
  const pitch = (k: number) => +(((at(k) + at(k + 1)) / 2) * (180 / Math.PI)).toFixed(4);
  const breaks: PitchBreak[] = [];
  for (let k = 1; k < n; k++) {
    breaks.push({ height: +(centre + r * Math.cos(at(k))).toFixed(4), pitch_deg: pitch(k) });
  }
  return { radius: r, ridge: centre + r, length: 2 * r * half, segments: n, pitch_deg: pitch(0), breaks };
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
