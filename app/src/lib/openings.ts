// Chimneys, roof windows and roof openings (DACAD: Schornstein,
// Dachfenster, Öffnung) as the editor places and edits them.

import type { CalcResult, Chimney, Point2, Project, RoofWindow } from "./types";
import { slopeAt } from "./dormers";

export type OpeningKind = "chimney" | "window" | "hole";
export type OpeningSel = { kind: OpeningKind; index: number } | null;

/** The project's list of openings of a kind. */
export function openingList(project: Project, kind: OpeningKind): (Chimney | RoofWindow)[] {
  return kind === "chimney" ? project.chimneys : kind === "window" ? (project.windows ?? []) : (project.roof_openings ?? []);
}

/** Position and size of a chimney or roof opening as the object bar shows
 *  them: the smallest box around its plan polygon that lines up with its
 *  first side. Presentation only, so the fields follow a chimney while it is
 *  dragged (ARCH-004, docs/adr): editing a field, and creating a chimney, go
 *  through the core (`api.reshapeOpening`, `api.chimneyFromRect`). */
export interface OpeningBox {
  x: number;
  y: number;
  width: number;
  depth: number;
  rotation: number;
}

const r3 = (v: number) => Math.round(v * 1e3) / 1e3;
const r4 = (v: number) => Math.round(v * 1e4) / 1e4;

interface Frame {
  origin: Point2;
  u: Point2;
  v: Point2;
  width: number;
  depth: number;
  unit: [number, number][];
}

function frame(k: Point2[]): Frame | null {
  const n = k.length;
  if (n < 3 || !k.every((c) => Number.isFinite(c.x) && Number.isFinite(c.y))) return null;
  let u: Point2 | null = null;
  for (let i = 0; i < n && !u; i++) {
    const dx = k[(i + 1) % n].x - k[i].x, dy = k[(i + 1) % n].y - k[i].y;
    const len = Math.hypot(dx, dy);
    if (len > 1e-6) u = { x: dx / len, y: dy / len };
  }
  if (!u) return null;
  const v = { x: -u.y, y: u.x };
  const o = k[0];
  const st = k.map((c) => {
    const x = c.x - o.x, y = c.y - o.y;
    return [x * u.x + y * u.y, x * v.x + y * v.y] as [number, number];
  });
  const s0 = Math.min(...st.map((p) => p[0])), s1 = Math.max(...st.map((p) => p[0]));
  const t0 = Math.min(...st.map((p) => p[1])), t1 = Math.max(...st.map((p) => p[1]));
  const width = s1 - s0, depth = t1 - t0;
  if (width <= 1e-6 || depth <= 1e-6) return null;
  return {
    origin: { x: o.x + u.x * s0 + v.x * t0, y: o.y + u.y * s0 + v.y * t0 },
    u,
    v,
    width,
    depth,
    unit: st.map(([s, t]) => [(s - s0) / width, (t - t0) / depth]),
  };
}

/** The box of a chimney or roof opening; null when it has no area. */
export function openingBox(corners: Point2[]): OpeningBox | null {
  const f = frame(corners);
  if (!f) return null;
  return { x: r3(f.origin.x), y: r3(f.origin.y), width: r3(f.width), depth: r3(f.depth), rotation: r4((Math.atan2(f.u.y, f.u.x) * 180) / Math.PI) };
}

/** Plan outline of a roof window lying in the face under its corner. */
export function windowOutline(result: CalcResult | null, w: RoofWindow): Point2[] {
  const s = slopeAt(result, { x: w.x + 1e-3, y: w.y + 1e-3 }) ?? { up: { x: 0, y: 1 }, tan: 0 };
  const along = { x: s.up.y, y: -s.up.x };
  // Height runs up the slope in the roof plane; in plan it is shortened.
  const h = w.height / Math.sqrt(1 + s.tan * s.tan);
  const o = { x: w.x, y: w.y };
  return [
    o,
    { x: o.x + along.x * w.width, y: o.y + along.y * w.width },
    { x: o.x + along.x * w.width + s.up.x * h, y: o.y + along.y * w.width + s.up.y * h },
    { x: o.x + s.up.x * h, y: o.y + s.up.y * h },
  ];
}

/** True when a plan polygon turns the same way at every corner. DACAD
 *  calculates no roof at all once a chimney or roof opening is non-convex. */
export function isConvex(corners: Point2[]): boolean {
  const n = corners.length;
  if (n < 3) return false;
  let sign = 0;
  for (let i = 0; i < n; i++) {
    const a = corners[i], b = corners[(i + 1) % n], c = corners[(i + 2) % n];
    const cross = (b.x - a.x) * (c.y - b.y) - (b.y - a.y) * (c.x - b.x);
    if (Math.abs(cross) < 1e-9) continue;
    if (sign === 0) sign = Math.sign(cross);
    else if (Math.sign(cross) !== sign) return false;
  }
  return sign !== 0;
}

/** Chimneys and roof openings that keep DACAD from calculating the roof. */
export function nonConvexOpenings(project: Project): { kind: OpeningKind; index: number }[] {
  const bad: { kind: OpeningKind; index: number }[] = [];
  project.chimneys.forEach((c, index) => isConvex(c.corners) || bad.push({ kind: "chimney", index }));
  (project.roof_openings ?? []).forEach((c, index) => isConvex(c.corners) || bad.push({ kind: "hole", index }));
  return bad;
}
