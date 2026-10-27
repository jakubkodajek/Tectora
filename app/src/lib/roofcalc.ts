// DACAD's calculation helpers (Parameter > Neigungen / Traufenhöhen /
// Vorsprünge berechnen; DACAD.HLP): they solve a wall's plane for its pitch,
// eave height or overhang from given measures. A wall's plane rises from its
// eave line, which lies at the overhang line (the engine's convention):
// z = eave + tan(pitch) · (inward distance from the wall + overhang).

import type { CalcResult, EdgeDef, Point2 } from "./types";
import { isGable, isMarker } from "./types";

export type CalcTarget = "pitch" | "eave" | "overhang";

const DEG = Math.PI / 180;
const tan = (deg: number) => Math.tan(deg * DEG);
const atanDeg = (v: number) => Math.atan(v) / DEG;
const round = (v: number, d = 4) => +v.toFixed(d);

/** Wall `i`'s line: start, unit direction and inward normal (the roof rises to the left). */
export function wallFrame(edges: EdgeDef[], i: number) {
  const a = edges[i].start;
  const b = edges[(i + 1) % edges.length].start;
  const l = Math.hypot(b.x - a.x, b.y - a.y) || 1;
  const u = { x: (b.x - a.x) / l, y: (b.y - a.y) / l };
  return { a, u, n: { x: -u.y, y: u.x } };
}

/** Inward distance of `p` from wall `i`'s line. */
export function inward(edges: EdgeDef[], i: number, p: Point2): number {
  const f = wallFrame(edges, i);
  return (p.x - f.a.x) * f.n.x + (p.y - f.a.y) * f.n.y;
}

/**
 * Einfaches Dreieck: two of plan width (wall to top, overhang included when
 * `withOverhang`), rafter length and rise give the pitch (Pythagoras).
 */
export function triangle(
  e: EdgeDef,
  given: { width?: number; rafter?: number; rise?: number },
  withOverhang: boolean,
): { pitch_deg: number; width: number; rafter: number; rise: number } | null {
  const ov = withOverhang ? 0 : e.overhang;
  let { width, rafter, rise } = given;
  if (width != null) width += ov;
  if (width != null && rise != null) rafter = Math.hypot(width, rise);
  else if (width != null && rafter != null) {
    if (rafter <= width) return null;
    rise = Math.sqrt(rafter * rafter - width * width);
  } else if (rafter != null && rise != null) {
    if (rafter <= rise) return null;
    width = Math.sqrt(rafter * rafter - rise * rise);
  } else return null;
  if (!(width! > 0) || !(rise! > 0)) return null;
  return { pitch_deg: round(atanDeg(rise! / width!)), width: width! - ov, rafter: rafter!, rise: rise! };
}

export interface ParallelInput {
  /** Distance between the two wall lines. */
  span: number;
  /** Plan width from wall A to the ridge, without overhang. */
  widthA: number;
  ridge: number;
  target: CalcTarget;
}

/**
 * 2 Parallele: two facing walls meet in a ridge at `widthA` from wall A and
 * height `ridge`; solves both walls for the target, keeping the rest.
 */
export function parallels(a: EdgeDef, b: EdgeDef, p: ParallelInput): [Partial<EdgeDef>, Partial<EdgeDef>] | null {
  const runs = [p.widthA, p.span - p.widthA];
  if (!(runs[0] > 0) || !(runs[1] > 0)) return null;
  const out = [a, b].map((e, k) => solve(e, runs[k], p.ridge, p.target));
  return out[0] && out[1] ? [out[0], out[1]] : null;
}

/**
 * The wall's roof profile as segments from the overhang line: each starts at
 * run `x` (distance from the overhang line) and height `z` with its pitch.
 */
function profile(e: EdgeDef): { x: number; z: number; t: number }[] {
  const segs = [{ x: 0, z: e.eave_height, t: tan(e.pitch_deg) }];
  for (const b of e.breaks ?? []) {
    const last = segs[segs.length - 1];
    if (b.height <= last.z || !(last.t > 0)) break;
    segs.push({ x: last.x + (b.height - last.z) / last.t, z: b.height, t: tan(b.pitch_deg) });
  }
  return segs;
}

/**
 * Solves wall `e` so that its roof reaches height `z` at inward distance `d`.
 * With pitch breaks, the pitch is that of the last break (DACAD.HLP,
 * Einfaches Dreieck); a new eave height moves the breaks with it.
 */
export function solve(e: EdgeDef, d: number, z: number, target: CalcTarget): Partial<EdgeDef> | null {
  if (isGable(e)) return null;
  const segs = profile(e);
  const run = d + e.overhang;
  if (target === "pitch") {
    const last = segs[segs.length - 1];
    if (!(run > last.x) || !(z > last.z)) return null;
    const pitch = round(atanDeg((z - last.z) / (run - last.x)));
    if (segs.length === 1) return { pitch_deg: pitch };
    const breaks = (e.breaks ?? []).map((b) => ({ ...b }));
    breaks[segs.length - 2].pitch_deg = pitch;
    return { breaks };
  }
  // Height of the current roof at run x, and the run where it reaches height h.
  const heightAt = (x: number) => {
    const s = [...segs].reverse().find((g) => x >= g.x) ?? segs[0];
    return s.z + s.t * (x - s.x);
  };
  const runAt = (h: number) => {
    const s = [...segs].reverse().find((g) => h >= g.z) ?? segs[0];
    return s.t > 0 ? s.x + (h - s.z) / s.t : NaN;
  };
  if (target === "eave") {
    const dz = z - heightAt(run);
    return {
      eave_height: round(e.eave_height + dz),
      ...(e.breaks?.length ? { breaks: e.breaks.map((b) => ({ ...b, height: round(b.height + dz) })) } : {}),
    };
  }
  const o = runAt(z) - d;
  return o >= -1e-9 ? { overhang: round(Math.max(0, o)) } : null;
}

/** Height of wall `e`'s roof profile at inward distance `d`. */
export function roofHeight(e: EdgeDef, d: number): number {
  const segs = profile(e);
  const x = d + e.overhang;
  const g = [...segs].reverse().find((s) => x >= s.x) ?? segs[0];
  return g.z + g.t * (x - g.x);
}

/** Where two facing walls `span` apart meet: plan width from wall A and height. */
export function meeting(a: EdgeDef, b: EdgeDef, span: number): { w: number; h: number } | null {
  const f = (w: number) => roofHeight(a, w) - roofHeight(b, span - w);
  let lo = 0;
  let hi = span;
  if (f(lo) > 0 || f(hi) < 0) return null;
  for (let k = 0; k < 100; k++) {
    const mid = (lo + hi) / 2;
    if (f(mid) < 0) lo = mid;
    else hi = mid;
  }
  return { w: lo, h: roofHeight(a, lo) };
}

/** Rafter length (overhang included) and plan width of wall `e` up to inward distance `d`. */
export const rafter = (e: EdgeDef, d: number) => (d + e.overhang) / Math.cos(e.pitch_deg * DEG);

/**
 * Flächen zusammenfassen: walls parallel to the reference wall `ref` (same
 * direction) take its plane, solved for the target.
 */
export function mergeOnto(edges: EdgeDef[], ref: number, walls: number[], target: CalcTarget): Map<number, Partial<EdgeDef>> {
  const r = edges[ref];
  const out = new Map<number, Partial<EdgeDef>>();
  for (const i of walls) {
    if (i === ref) continue;
    const e = edges[i];
    // A point on wall i's line at inward distance 0 lies at s from the reference line.
    const s = inward(edges, ref, e.start);
    const z = r.eave_height + tan(r.pitch_deg) * (s + r.overhang);
    const patch: Partial<EdgeDef> = { pitch_deg: r.pitch_deg };
    if (target === "eave") patch.eave_height = round(z - tan(r.pitch_deg) * e.overhang);
    else if (target === "overhang") {
      const o = (z - e.eave_height) / tan(r.pitch_deg);
      if (o < -1e-9) continue;
      patch.overhang = round(Math.max(0, o));
    } else continue;
    out.set(i, patch);
  }
  return out;
}

/** Walls of the outline parallel to wall `i` and facing the same way. */
export function sameWay(edges: EdgeDef[], i: number): number[] {
  const f = wallFrame(edges, i);
  return edges.flatMap((e, k) => {
    if (isMarker(e) || isGable(e)) return [];
    const g = wallFrame(edges, k);
    const len = Math.hypot(edges[(k + 1) % edges.length].start.x - e.start.x, edges[(k + 1) % edges.length].start.y - e.start.y);
    return len > 1e-6 && g.u.x * f.u.x + g.u.y * f.u.y > 1 - 1e-6 ? [k] : [];
  });
}

/** Roof line end points of a result (ridge, hip and valley corners), deduplicated. */
export function roofPoints(result: CalcResult | null): { p: [number, number, number]; kind: string }[] {
  if (!result) return [];
  const seen = new Map<string, { p: [number, number, number]; kind: string }>();
  for (const part of result.parts) {
    for (const l of part.geometry.lines) {
      if (l.kind === "eave") continue;
      for (const q of [l.a, l.b]) {
        const key = q.map((v) => v.toFixed(3)).join(",");
        if (!seen.has(key)) seen.set(key, { p: [q[0], q[1], q[2]], kind: l.kind });
      }
    }
  }
  return [...seen.values()].sort((x, y) => y.p[2] - x.p[2]);
}

/** Inward shift of wall `e` that makes its roof reach height `z` at inward distance `d` (DACAD: Lage berechnen). */
export function shiftFor(e: EdgeDef, d: number, z: number): number | null {
  if (isGable(e)) return null;
  const segs = profile(e);
  const s = [...segs].reverse().find((g) => z >= g.z) ?? segs[0];
  if (!(s.t > 0) || z < segs[0].z) return null;
  return d - (s.x + (z - s.z) / s.t - e.overhang);
}

/**
 * Moves wall `i` parallel by `delta` inward; its ends slide along the
 * neighbouring walls (or move with it where a neighbour is parallel).
 * Zero-length marker walls at a moved corner move with it.
 */
export function moveWall(edges: EdgeDef[], i: number, delta: number): Map<number, Partial<EdgeDef>> {
  const n = edges.length;
  const at = (k: number) => edges[((k % n) + n) % n].start;
  const short = (k: number) => Math.hypot(at(k + 1).x - at(k).x, at(k + 1).y - at(k).y) < 1e-6;
  const f = wallFrame(edges, i);
  const a = { x: f.a.x + f.n.x * delta, y: f.a.y + f.n.y * delta };
  const meet = (p: Point2, q: Point2, fallback: Point2) => {
    // The moved line (through a, direction u) meets the line p→q.
    const v = { x: q.x - p.x, y: q.y - p.y };
    const den = f.u.x * v.y - f.u.y * v.x;
    if (Math.abs(den) < 1e-9 * Math.max(1e-9, Math.hypot(v.x, v.y))) return fallback;
    const t = ((p.x - a.x) * v.y - (p.y - a.y) * v.x) / den;
    return { x: round(a.x + f.u.x * t, 3), y: round(a.y + f.u.y * t, 3) };
  };
  const shifted = (p: Point2) => ({ x: round(p.x + f.n.x * delta, 3), y: round(p.y + f.n.y * delta, 3) });
  const out = new Map<number, Partial<EdgeDef>>();
  // Start corner: the previous real wall, skipping markers that share the corner.
  const startIdx = [i];
  let k = i - 1;
  while (short(k) && startIdx.length < n) startIdx.push(k--);
  const s = meet(at(k), at(k + 1), shifted(at(i)));
  for (const j of startIdx) out.set(((j % n) + n) % n, { start: s });
  // End corner: the next real wall, likewise.
  const endIdx = [i + 1];
  k = i + 1;
  while (short(k) && endIdx.length < n) endIdx.push(++k);
  const e = meet(at(k), at(k + 1), shifted(at(i + 1)));
  for (const j of endIdx) out.set(((j % n) + n) % n, { start: e });
  return out;
}

/** Merges patch maps; later fields win. */
export function combine(...maps: Map<number, Partial<EdgeDef>>[]): Map<number, Partial<EdgeDef>> {
  const out = new Map<number, Partial<EdgeDef>>();
  for (const m of maps) for (const [k, v] of m) out.set(k, { ...(out.get(k) ?? {}), ...v });
  return out;
}

/**
 * Grat/Kehle: from the natural measures of an existing roof (hip or valley
 * length and the two rafter lengths, both from the eave lines) the pitches
 * of two neighbouring walls `i` and `j` with the same eave height.
 */
export function hipPitches(edges: EdgeDef[], i: number, j: number, hip: number, r1: number, r2: number): Map<number, Partial<EdgeDef>> | null {
  const n = edges.length;
  // Order so that wall `first` ends where wall `second` starts.
  const [first, second, ra, rb] = (i + 1) % n === j ? [i, j, r1, r2] : [j, i, r2, r1];
  if ((first + 1) % n !== second) return null;
  const e1 = edges[first];
  const e2 = edges[second];
  if (isGable(e1) || isGable(e2) || Math.abs(e1.eave_height - e2.eave_height) > 1e-3) return null;
  const u1 = wallFrame(edges, first).u;
  const u2 = wallFrame(edges, second).u;
  // Interior angle at the corner; above 180° the line is a valley.
  const alpha = Math.PI - Math.atan2(u1.x * u2.y - u1.y * u2.x, u1.x * u2.x + u1.y * u2.y);
  const s2 = Math.sin(alpha) ** 2;
  if (s2 < 1e-9) return null;
  const f = (h: number) => {
    const d1 = Math.sqrt(Math.max(0, ra * ra - h * h));
    const d2 = Math.sqrt(Math.max(0, rb * rb - h * h));
    return (d1 * d1 + d2 * d2 + 2 * d1 * d2 * Math.cos(alpha)) / s2 + h * h - hip * hip;
  };
  let lo = 0;
  let hi = Math.min(ra, rb);
  if (!(hi > 0) || f(lo) < 0 || f(hi) > 0) return null;
  for (let k = 0; k < 200; k++) {
    const mid = (lo + hi) / 2;
    if (f(mid) > 0) lo = mid;
    else hi = mid;
  }
  const h = (lo + hi) / 2;
  const pitch = (r: number) => round(atanDeg(h / Math.sqrt(r * r - h * h)));
  return new Map([
    [first, { pitch_deg: pitch(ra) }],
    [second, { pitch_deg: pitch(rb) }],
  ]);
}
