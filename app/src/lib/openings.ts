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

/** A rectangular chimney: corner, size and rotation (degrees). */
export interface ChimneyRect {
  x: number;
  y: number;
  width: number;
  depth: number;
  rotation: number;
  height: number;
}

export function chimneyFromRect(r: ChimneyRect): Chimney {
  const a = (r.rotation * Math.PI) / 180;
  const u = { x: Math.cos(a), y: Math.sin(a) };
  const v = { x: -u.y, y: u.x };
  const p = (s: number, t: number) => ({
    x: +(r.x + u.x * s + v.x * t).toFixed(3),
    y: +(r.y + u.y * s + v.y * t).toFixed(3),
  });
  return { corners: [p(0, 0), p(r.width, 0), p(r.width, r.depth), p(0, r.depth)], height: r.height };
}

/** Reads a four-cornered rectangular chimney back; others return null. */
export function rectFromChimney(c: Chimney): ChimneyRect | null {
  const k = c.corners;
  if (k.length !== 4) return null;
  const d = (i: number) => ({ x: k[(i + 1) % 4].x - k[i].x, y: k[(i + 1) % 4].y - k[i].y });
  const [e0, e1, e2] = [d(0), d(1), d(2)];
  const dot = (a: Point2, b: Point2) => a.x * b.x + a.y * b.y;
  const w = Math.hypot(e0.x, e0.y);
  const h = Math.hypot(e1.x, e1.y);
  if (w < 1e-6 || h < 1e-6 || Math.abs(dot(e0, e1)) > 1e-4 || Math.abs(e0.x + e2.x) + Math.abs(e0.y + e2.y) > 1e-3) return null;
  // Counter-clockwise from the first corner, as chimneyFromRect builds it.
  if (e0.x * e1.y - e0.y * e1.x <= 0) return null;
  return { x: k[0].x, y: k[0].y, width: +w.toFixed(3), depth: +h.toFixed(3), rotation: +((Math.atan2(e0.y, e0.x) * 180) / Math.PI).toFixed(1), height: c.height };
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
