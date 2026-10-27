// DACAD's dormer types (Gaubenarten) as parameter sets over the engine's
// dormer: an open wall chain of a zero-length cheek, the front wall and a
// second zero-length cheek, as DACAD stores it (cases C29-C31, C39, C76).

import type { CalcResult, Dormer, EdgeDef, Point2 } from "./types";
import { GABLE_PITCH, isGable } from "./types";

export type DormerKind = "gable" | "pointed" | "hip" | "half_hip" | "shed" | "trapezoid" | "general";

/** Kinds that can be placed; "general" is any other chain, kept as it is. */
export const DORMER_KINDS: DormerKind[] = ["gable", "pointed", "hip", "half_hip", "shed", "trapezoid"];

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
  overhang: number;
  /** Krüppelwalm: height and pitch of the clipped part of the front. */
  half_hip_height: number;
  half_hip_pitch_deg: number;
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
    half_hip_height: 0.6,
    half_hip_pitch_deg: 60,
  };
}

export function buildDormer(p: DormerParams): Dormer {
  const u = { x: Math.cos(p.angle), y: Math.sin(p.angle) };
  const end = { x: +(p.start.x + u.x * p.width).toFixed(3), y: +(p.start.y + u.y * p.width).toFixed(3) };
  const sloped = p.kind !== "shed";
  const frontSloped = p.kind === "hip" || p.kind === "shed" || p.kind === "trapezoid";
  const base = { eave_height: p.eave_height, overhang: p.overhang };
  const cheek = (dir: number, start: Point2): EdgeDef => ({
    ...base,
    start: { ...start },
    pitch_deg: sloped ? p.pitch_deg : GABLE_PITCH,
    direction_rad: norm(dir),
  });
  const front: EdgeDef = {
    ...base,
    start: { ...p.start },
    pitch_deg: frontSloped ? (p.kind === "hip" ? p.pitch_deg : p.front_pitch_deg) : GABLE_PITCH,
    half_hip:
      p.kind === "half_hip" ? { height: +(p.eave_height + p.half_hip_height).toFixed(3), pitch_deg: p.half_hip_pitch_deg } : null,
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
  };
  if (d.edges.length !== 3 || a.direction_rad == null || b.direction_rad == null || f.direction_rad != null) return p;
  const cheeksGable = isGable(a) && isGable(b);
  const cheeksSloped = !isGable(a) && !isGable(b) && Math.abs(a.pitch_deg - b.pitch_deg) < 1e-6;
  if (cheeksGable && !isGable(f)) {
    return { ...p, kind: "shed", front_pitch_deg: f.pitch_deg };
  }
  if (!cheeksSloped) return p;
  p.pitch_deg = a.pitch_deg;
  if (isGable(f)) {
    if (f.half_hip) {
      return { ...p, kind: "half_hip", half_hip_height: +(f.half_hip.height - f.eave_height).toFixed(3), half_hip_pitch_deg: f.half_hip.pitch_deg };
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
