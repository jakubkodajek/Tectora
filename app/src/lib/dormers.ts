// DACAD's dormer types (Gaubenarten) as parameter sets over the engine's
// dormer: an open wall chain of a zero-length cheek, the front wall and a
// second zero-length cheek, as DACAD stores it (cases C29-C31, C39, C76).

import type { CalcResult, Dormer, EdgeDef, Point2 } from "./types";
import { GABLE_PITCH, isGable } from "./types";
import { MAX_ROUND_SEGMENTS, arcFor } from "./roundroof";
import { roundTo } from "./rounding";

export type DormerKind = "gable" | "pointed" | "hip" | "half_hip" | "shed" | "trapezoid" | "round" | "general";

/** Kinds that can be placed; "general" is any other chain, kept as it is. */
export const DORMER_KINDS: DormerKind[] = ["gable", "pointed", "hip", "half_hip", "shed", "trapezoid", "round"];

export interface DormerParams {
  kind: DormerKind;
  /** Left corner of the front, seen from outside. */
  start: Point2;
  width: number;
  /** Direction of the front wall, radians (the roof rises to its left). */
  angle: number;
  eave_height: number;
  /** Pitch of the cheek planes (gable, hip, half-hip, trapezoid). */
  pitch_deg: number;
  /** Pitch of the front plane (hip, shed, trapezoid). */
  front_pitch_deg: number;
  /** Overhang of the front. */
  overhang: number;
  /** Overhang of the cheeks (the dormer roof is width + 2 × this wide). */
  side_overhang: number;
  /** Krüppelwalm: height and pitch of the clipped part of the front. */
  half_hip_height: number;
  half_hip_pitch_deg: number;
  /** Rundgaube: polygon segments per cheek; 0 = DACAD's count. */
  segments: number;
  /** Roof material of all the dormer's walls. */
  material: number;
}

export function defaultParams(kind: DormerKind, start: Point2, angle: number, eave: number): DormerParams {
  return {
    kind,
    start,
    width: 2,
    angle,
    eave_height: eave,
    pitch_deg: 45,
    front_pitch_deg: kind === "shed" || kind === "trapezoid" ? 15 : 45,
    overhang: 0,
    side_overhang: 0,
    half_hip_height: 0.6,
    half_hip_pitch_deg: 60,
    segments: 0,
    material: 1,
  };
}

export function buildDormer(p: DormerParams): Dormer {
  const u = { x: Math.cos(p.angle), y: Math.sin(p.angle) };
  const end = { x: roundTo(p.start.x + u.x * p.width, 3), y: roundTo(p.start.y + u.y * p.width, 3) };
  const sloped = p.kind !== "shed";
  const frontSloped = p.kind === "hip" || p.kind === "shed" || p.kind === "trapezoid";
  const base = { eave_height: p.eave_height, ...(p.material !== 1 ? { material: p.material } : {}) };
  // A Rundgaube is a gable dormer whose cheeks follow the Runddach arc (DACAD.HLP);
  // DACAD's default arc is assumed, its dormer dialog is not verified yet.
  const arc =
    p.kind === "round"
      ? arcFor({ span: p.width, overhang: p.side_overhang, eave_height: p.eave_height, segments: Math.min(MAX_ROUND_SEGMENTS, p.segments), mode: "default", value: 0 })
      : null;
  const cheek = (dir: number, start: Point2): EdgeDef => ({
    ...base,
    overhang: p.side_overhang,
    start: { ...start },
    pitch_deg: arc ? arc.pitch_deg : sloped ? p.pitch_deg : GABLE_PITCH,
    direction_rad: norm(dir),
    ...(arc ? { breaks: arc.breaks.map((b) => ({ ...b })) } : {}),
  });
  const front: EdgeDef = {
    ...base,
    overhang: p.overhang,
    start: { ...p.start },
    pitch_deg: frontSloped ? (p.kind === "hip" ? p.pitch_deg : p.front_pitch_deg) : GABLE_PITCH,
    half_hip:
      p.kind === "half_hip" ? { height: roundTo(p.eave_height + p.half_hip_height, 3), pitch_deg: p.half_hip_pitch_deg } : null,
  };
  return {
    edges: [cheek(p.angle - Math.PI / 2, p.start), front, cheek(p.angle + Math.PI / 2, end)],
    end,
  };
}

/** Reads a dormer back as parameters; any other chain is "general". */
export function parseDormer(d: Dormer): DormerParams {
  const [a, f, b] = d.edges;
  const start = f?.start ?? d.edges[0]?.start ?? d.end;
  const angle = Math.atan2(d.end.y - start.y, d.end.x - start.x);
  const p: DormerParams = {
    ...defaultParams("general", start, angle, f?.eave_height ?? d.edges[0]?.eave_height ?? 0),
    width: Math.hypot(d.end.x - start.x, d.end.y - start.y),
    overhang: f?.overhang ?? 0,
    side_overhang: (d.edges[0] ?? f)?.overhang ?? 0,
    material: f?.material ?? 1,
  };
  if (d.edges.length !== 3 || a.direction_rad == null || b.direction_rad == null || f.direction_rad != null) return p;
  const cheeksGable = isGable(a) && isGable(b);
  const cheeksSloped = !isGable(a) && !isGable(b) && Math.abs(a.pitch_deg - b.pitch_deg) < 1e-6;
  if (cheeksGable && !isGable(f)) {
    return { ...p, kind: "shed", front_pitch_deg: f.pitch_deg };
  }
  if (!cheeksSloped) return p;
  p.pitch_deg = a.pitch_deg;
  if (isGable(f) && (a.breaks?.length ?? 0) > 0) {
    return { ...p, kind: "round", segments: (a.breaks?.length ?? 0) + 1 };
  }
  if (isGable(f)) {
    if (f.half_hip) {
      return { ...p, kind: "half_hip", half_hip_height: roundTo(f.half_hip.height - f.eave_height, 3), half_hip_pitch_deg: f.half_hip.pitch_deg };
    }
    return { ...p, kind: "gable" };
  }
  return Math.abs(f.pitch_deg - a.pitch_deg) < 1e-6 ? { ...p, kind: "hip" } : { ...p, kind: "trapezoid", front_pitch_deg: f.pitch_deg };
}

function norm(a: number) {
  const t = 2 * Math.PI;
  return ((a % t) + t) % t;
}

/** Height of the calculated roof at a plan point, or null off the roof. */
export function roofHeightAt(result: CalcResult | null, p: Point2): number | null {
  if (!result) return null;
  let best: number | null = null;
  for (const part of result.parts) {
    for (const f of part.geometry.faces) {
      const poly = f.polygon;
      if (poly.length < 3 || !inside(poly, p)) continue;
      const z = planeZ(poly, p);
      if (z !== null && (best === null || z > best)) best = z;
    }
  }
  return best;
}

/** Dormer faces belong to no wall: the engine gives them usize::MAX. */
export const isDormerFace = (edge: number) => edge > 1e15;

/** Height of the roof a dormer or chimney stands on (without the dormers'
 *  own faces), as tectora-core `edit::probe::host_height_at`. */
export function hostHeightAt(result: CalcResult | null, p: Point2): number | null {
  if (!result) return null;
  let best: number | null = null;
  for (const part of result.parts) {
    for (const f of part.geometry.faces) {
      const poly = f.polygon;
      if (isDormerFace(f.edge) || poly.length < 3 || !inside(poly, p)) continue;
      const z = planeZ(poly, p);
      if (z !== null && (best === null || z > best)) best = z;
    }
  }
  return best;
}

function inside(poly: [number, number, number][], p: Point2) {
  let c = false;
  for (let i = 0, j = poly.length - 1; i < poly.length; j = i++) {
    const [xi, yi] = poly[i];
    const [xj, yj] = poly[j];
    if (yi > p.y !== yj > p.y && p.x < ((xj - xi) * (p.y - yi)) / (yj - yi) + xi) c = !c;
  }
  return c;
}

function planeZ(poly: [number, number, number][], p: Point2): number | null {
  const o = poly[0];
  for (let i = 1; i + 1 < poly.length; i++) {
    const a = poly[i];
    const b = poly[i + 1];
    const u = [a[0] - o[0], a[1] - o[1], a[2] - o[2]];
    const v = [b[0] - o[0], b[1] - o[1], b[2] - o[2]];
    const n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
    if (Math.abs(n[2]) > 1e-9) return o[2] - (n[0] * (p.x - o[0]) + n[1] * (p.y - o[1])) / n[2];
  }
  return null;
}

/** Direction of the outline wall nearest to `p` (the front runs along it). */
export function nearestWallAngle(parts: { edges: EdgeDef[] }[], p: Point2): number {
  let best = { d: Infinity, a: 0 };
  for (const part of parts) {
    const es = part.edges;
    es.forEach((e, i) => {
      const b = es[(i + 1) % es.length].start;
      const dx = b.x - e.start.x;
      const dy = b.y - e.start.y;
      const l2 = dx * dx + dy * dy;
      if (l2 < 1e-12 || isGable(e)) return;
      const t = Math.max(0, Math.min(1, ((p.x - e.start.x) * dx + (p.y - e.start.y) * dy) / l2));
      const d = Math.hypot(e.start.x + t * dx - p.x, e.start.y + t * dy - p.y);
      if (d < best.d) best = { d, a: Math.atan2(dy, dx) };
    });
  }
  return best.a;
}

/** Uphill direction (unit, plan) and pitch tangent of the roof face at `p`. */
export function slopeAt(result: CalcResult | null, p: Point2): { up: Point2; tan: number } | null {
  if (!result) return null;
  let best: { z: number; up: Point2; tan: number } | null = null;
  for (const part of result.parts) {
    for (const f of part.geometry.faces) {
      const poly = f.polygon;
      if (poly.length < 3 || !inside(poly, p)) continue;
      const z = planeZ(poly, p);
      if (z === null || (best && z <= best.z)) continue;
      const zx = (planeZ(poly, { x: p.x + 1, y: p.y }) ?? z) - z;
      const zy = (planeZ(poly, { x: p.x, y: p.y + 1 }) ?? z) - z;
      const tan = Math.hypot(zx, zy);
      best = { z, up: tan > 1e-9 ? { x: zx / tan, y: zy / tan } : { x: 0, y: 1 }, tan };
    }
  }
  return best && { up: best.up, tan: best.tan };
}

/** The roof a dormer stands on at the middle of its front: height and rise
 *  per metre into the roof (tectora-core `edit::dormers::HostSlope`). */
export interface HostSlope {
  height: number;
  tan: number;
}

/** The side section of the dormer dialog, as tectora-core `edit::dormers::section`. */
export interface DormerSection {
  host_pitch_deg: number;
  wall_height: number;
  ridge_height: number | null;
  depth: number | null;
  top_height: number | null;
  ridge_length: number | null;
  roof_length: number | null;
  host_length: number | null;
  /** Trapezoid: width of the shed roof where it meets the roof (0 when the cheeks close in a ridge). */
  back_width: number | null;
}

export type SectionTarget = { wall_height: number } | { depth: number } | { ridge_length: number };

const MIN_TAN = 1e-3;
const tanDeg = (d: number) => Math.tan((d * Math.PI) / 180);
const cosDeg = (d: number) => Math.cos((d * Math.PI) / 180);

/** Rise and slope length of a wall's profile over a horizontal run. */
function profile(e: EdgeDef, run: number): [number, number] {
  let z = e.eave_height;
  let x = 0;
  let len = 0;
  let pitch = e.pitch_deg;
  const breaks = (e.breaks ?? []).filter((b) => b.height > z).sort((a, b) => a.height - b.height);
  for (const b of breaks) {
    const t = tanDeg(pitch);
    if (t <= 0) break;
    const dx = (b.height - z) / t;
    if (x + dx >= run) break;
    len += dx / cosDeg(pitch);
    x += dx;
    z = b.height;
    pitch = b.pitch_deg;
  }
  const rest = run - x;
  return [z + rest * tanDeg(pitch) - e.eave_height, len + rest / cosDeg(pitch)];
}

function cheekOf(p: DormerParams): EdgeDef | null {
  if (p.kind === "shed" || p.kind === "general") return null;
  return buildDormer(p).edges[0];
}

/** Height of the dormer's wall top above its eave (the eave is at the overhang's
 *  edge): a shed dormer's front wall, the other kinds' cheek walls. */
function wallRise(p: DormerParams): number {
  const c = cheekOf(p);
  return c ? profile(c, p.side_overhang)[0] : p.overhang * tanDeg(p.front_pitch_deg);
}

function ridgeRise(p: DormerParams): [number, number] | null {
  const c = cheekOf(p);
  if (!c) return null;
  const [rise, len] = profile(c, p.width / 2 + p.side_overhang);
  return [rise, len - profile(c, p.side_overhang)[1]];
}

/** Rust's f64::max: NaN gives the other value. */
const fmax = (a: number, b: number) => (Number.isNaN(a) ? b : Number.isNaN(b) ? a : Math.max(a, b));

function ridgeStart(p: DormerParams, rise: number): number {
  const ov = p.overhang;
  let start: number;
  switch (p.kind) {
    case "hip":
      start = rise / tanDeg(p.pitch_deg) - ov;
      break;
    case "trapezoid":
      start = rise / tanDeg(p.front_pitch_deg) - ov;
      break;
    case "half_hip":
      start = (rise - p.half_hip_height) / tanDeg(p.half_hip_pitch_deg) - ov;
      break;
    default:
      start = -ov;
  }
  return fmax(start, -ov);
}

const MIN_SHED_TAN = 1e-2;

/** Where a sloped front plane runs into the roof: [depth, rise above the eave]. */
function frontPlaneTop(p: DormerParams, host: HostSlope): [number, number] | null {
  const tf = tanDeg(p.front_pitch_deg);
  const wall = p.eave_height + p.overhang * tf - host.height;
  const k = host.tan - tf;
  if (!(k > MIN_SHED_TAN && wall >= -1e-6)) return null;
  const d = fmax(wall, 0) / k;
  return [d, (d + p.overhang) * tf];
}

/** A trapezoid dormer whose shed roof meets the roof before its cheeks close. */
function trapezoidTop(p: DormerParams, host: HostSlope): [number, number] | null {
  if (p.kind !== "trapezoid") return null;
  const rr = ridgeRise(p);
  if (!rr) return null;
  const t = frontPlaneTop(p, host);
  return t && t[1] < rr[0] ? t : null;
}

const backWidth = (p: DormerParams, rise: number) => p.width + 2 * p.side_overhang - (2 * rise) / tanDeg(p.pitch_deg);

/** The dialog's section for a dormer on `host` (tectora-core `section`). */
export function dormerSection(p: DormerParams, host: HostSlope): DormerSection {
  const wall = p.eave_height + wallRise(p) - host.height;
  const s: DormerSection = {
    host_pitch_deg: (Math.atan(host.tan) * 180) / Math.PI,
    wall_height: wall,
    ridge_height: null,
    depth: null,
    top_height: null,
    ridge_length: null,
    roof_length: null,
    host_length: null,
    back_width: null,
  };
  let depth: number | null = null;
  const trap = trapezoidTop(p, host);
  if (p.kind === "shed") {
    const t = frontPlaneTop(p, host);
    depth = t ? t[0] : null;
    s.roof_length = depth === null ? null : depth / cosDeg(p.front_pitch_deg);
  } else if (trap) {
    depth = trap[0];
    s.roof_length = depth / cosDeg(p.front_pitch_deg);
    s.back_width = backWidth(p, trap[1]);
  } else {
    const rr = ridgeRise(p);
    if (rr) {
      const [rise, cheek] = rr;
      const top = p.eave_height + rise - host.height;
      s.ridge_height = p.eave_height + rise;
      s.roof_length = cheek;
      depth = host.tan > MIN_TAN && top >= 0 ? top / host.tan : null;
      s.ridge_length = depth === null ? null : fmax(depth - ridgeStart(p, rise), 0);
      if (p.kind === "trapezoid" && depth !== null) s.back_width = 0;
    }
  }
  const finite = (v: number | null) => (v !== null && Number.isFinite(v) ? v : null);
  s.depth = finite(depth);
  s.ridge_length = finite(s.ridge_length);
  s.roof_length = finite(s.roof_length);
  s.back_width = finite(s.back_width);
  s.top_height = s.depth === null ? null : s.depth * host.tan;
  s.host_length = s.depth === null ? null : s.depth / Math.cos(Math.atan(host.tan));
  return s;
}

/** Why a section value cannot be reached (keys of `dormer.err.*`). */
export type SectionProblem = "negative" | "no_ridge" | "too_steep" | "flat_roof" | "below_roof" | "bad_pitch" | "bad_back_width";

function paramsOk(p: DormerParams): boolean {
  const ok = (v: number) => v > 0 && v < 90;
  const pitches =
    p.kind === "shed"
      ? ok(p.front_pitch_deg)
      : p.kind === "trapezoid"
        ? ok(p.pitch_deg) && ok(p.front_pitch_deg)
        : p.kind === "half_hip"
          ? ok(p.pitch_deg) && ok(p.half_hip_pitch_deg)
          : ok(p.pitch_deg) || p.kind === "round";
  const len = (v: number) => Number.isFinite(v) && v >= 0;
  return pitches && p.width > 0 && Number.isFinite(p.width) && len(p.overhang) && len(p.side_overhang);
}

/** The eave height for a section value (tectora-core `eave_for`), or why not. */
export function eaveFor(p: DormerParams, host: HostSlope, target: SectionTarget): number | SectionProblem {
  if (!paramsOk(p)) return "bad_pitch";
  let eave: number;
  if ("wall_height" in target) {
    const h = target.wall_height;
    if (!Number.isFinite(h) || h < 0) return "negative";
    eave = host.height + h - wallRise(p);
  } else {
    const ridge = "ridge_length" in target;
    const d = ridge ? target.ridge_length : target.depth;
    if (!Number.isFinite(d) || d <= 0) return "negative";
    const tf = tanDeg(p.front_pitch_deg);
    if (p.kind === "shed") {
      if (ridge) return "no_ridge";
      const k = host.tan - tf;
      if (k <= MIN_SHED_TAN) return "too_steep";
      eave = host.height + d * k - wallRise(p);
    } else {
      const rr = ridgeRise(p);
      if (!rr) return "no_ridge";
      if (host.tan <= MIN_TAN) return "flat_roof";
      const shedTop = (d + p.overhang) * tf;
      if (p.kind === "trapezoid" && !ridge && host.tan - tf > MIN_SHED_TAN && shedTop < rr[0]) {
        eave = host.height + d * host.tan - shedTop;
      } else {
        const depth = ridge ? d + ridgeStart(p, rr[0]) : d;
        eave = host.height + depth * host.tan - rr[0];
      }
    }
  }
  if (!Number.isFinite(eave)) return "bad_pitch";
  if (eave + wallRise(p) < host.height - 1e-9) return "below_roof";
  return roundTo(eave, 3);
}

/** The cheek pitch that gives a trapezoid dormer the back width `width`
 *  (tectora-core `pitch_for_back_width`), or why not. */
export function pitchForBackWidth(p: DormerParams, host: HostSlope, width: number): number | SectionProblem {
  if (p.kind !== "trapezoid") return "no_ridge";
  const eaveWidth = p.width + 2 * p.side_overhang;
  if (!Number.isFinite(width) || width <= 0 || width >= eaveWidth) return "bad_back_width";
  const q = { ...p, pitch_deg: 45 };
  if (!paramsOk(q)) return "bad_pitch";
  const t = frontPlaneTop(q, host);
  if (!t) return "too_steep";
  if (t[1] <= 1e-6) return "below_roof";
  const pitch = roundTo((Math.atan((2 * t[1]) / (eaveWidth - width)) * 180) / Math.PI, 2);
  if (!(pitch > 0 && pitch < 90)) return "bad_back_width";
  return pitch;
}

/** The dormer with `target` (its eave) and a trapezoid's `backWidth` (its
 *  cheek pitch) solved in turn until they settle (tectora-core `solve`). */
export function solveDormer(p: DormerParams, host: HostSlope, target: SectionTarget | null, backWidth: number | null): DormerParams | SectionProblem {
  let q = { ...p };
  for (let i = 0; i < 20; i++) {
    const before = [q.eave_height, q.pitch_deg];
    if (target) {
      const e = eaveFor(q, host, target);
      if (typeof e !== "number") return e;
      q = { ...q, eave_height: e };
    }
    if (backWidth !== null) {
      const pitch = pitchForBackWidth(q, host, backWidth);
      if (typeof pitch !== "number") return pitch;
      q = { ...q, pitch_deg: pitch };
    }
    if (!target || backWidth === null || (before[0] === q.eave_height && before[1] === q.pitch_deg)) break;
  }
  return q;
}
