// DACAD's Runddach helper (Parameter > Neigungen berechnen > Runddach): on
// two parallel walls with the same eave height it sets pitches and Knicke so
// that an inscribed polygon approximates a circular arc ending at the overhang
// tips (DACAD.HLP topic "Runddach", RNDROOF dialog bitmap). The numbers follow
// DACAD runs RD1-RD4 (/mnt/project-files/dacad-round/): the default arc spans
// 45° on each side, a side gets ceil(arc / 11.25°) segments (15° steps when
// such a segment's chord would be under 0.4 m, Rundgaube runs RGa-RGs in
// dacad-round/rundgaube/), and segment k has the pitch (k + ½)·arc / n.
//
// What is left of this file is the arc of the round dormer (`dormers.ts`
// still builds its own dormers; ARCH-004). The round-roof dialog gets its arc
// from the core (`api.roundArc`); the wall search and span moved there too.

import type { PitchBreak } from "./types";
import { roundTo } from "./rounding";

export type ArcMode = "default" | "radius" | "ridge" | "length";

/** DACAD's default arc on each side of the ridge, degrees. */
const DEFAULT_ARC = 45;
/** DACAD's segment angle, degrees, and the coarser one for short segments. */
const SEGMENT_ARC = 11.25;
const SHORT_SEGMENT_ARC = 15;
/** Below this chord, m, DACAD takes the coarser steps (between 0.388 and 0.402 in its runs). */
const MIN_CHORD = 0.4;

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
export function segmentsFor(deg: number, radius: number): number {
  const steps = (arc: number) => Math.max(1, Math.ceil(deg / arc - 1e-9));
  const n = steps(SEGMENT_ARC);
  const chord = 2 * radius * Math.sin(((deg / n) * Math.PI) / 360);
  return chord < MIN_CHORD ? steps(SHORT_SEGMENT_ARC) : n;
}

/** Most segments per side, as in the core (`edit::round::MAX_ROUND_SEGMENTS`). */
export const MAX_ROUND_SEGMENTS = 30;

/** The arc for the input, or null when the value is out of range. */
export function arcFor(a: ArcInput): Arc | null {
  const c = minRadius(a.span, a.overhang);
  if (!(c > 0) || a.segments < 0 || a.segments > MAX_ROUND_SEGMENTS) return null;
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
  const n = a.segments || segmentsFor((half * 180) / Math.PI, r);
  const centre = a.eave_height - Math.sqrt(Math.max(0, r * r - c * c));
  // Vertices at equal angles from the vertical; a chord's pitch is the angle
  // of the radius to its middle.
  const at = (k: number) => half * (1 - k / n);
  const pitch = (k: number) => roundTo(((at(k) + at(k + 1)) / 2) * (180 / Math.PI), 4);
  const breaks: PitchBreak[] = [];
  for (let k = 1; k < n; k++) {
    breaks.push({ height: roundTo(centre + r * Math.cos(at(k)), 4), pitch_deg: pitch(k) });
  }
  return { radius: r, ridge: centre + r, length: 2 * r * half, segments: n, pitch_deg: pitch(0), breaks };
}
