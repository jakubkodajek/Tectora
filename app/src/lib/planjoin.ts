// How joined roofs read in the plan: which stretches of a roof line run
// under a higher roof (drawn dashed), and which buildings are joined to
// which (named in the results). Display only: the roof itself, its lines
// and quantities come from the core.

import { hostHeightAt } from "./dormers";
import type { CalcResult, Point2, RoofPart } from "./types";

type P3 = [number, number, number];

export interface LinePiece {
  a: P3;
  b: P3;
  /** A higher roof face lies over this stretch. */
  hidden: boolean;
}

/** Samples per line; a line shorter than a few cm is never split. */
const SAMPLES = 24;
/** A face this much above the line covers it, m [no source: a few cm, below any real roof build-up]. */
const COVER_TOL = 0.05;

/**
 * Splits a roof line into the stretches seen from above and the stretches
 * under a higher roof face (e.g. a lower wing's ridge under the overhang of
 * the main house). `topAt` is the height of the highest roof face over a
 * plan point. O(SAMPLES · faces).
 */
export function splitByCover(a: P3, b: P3, topAt: (p: Point2) => number | null): LinePiece[] {
  const at = (t: number): P3 => [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t];
  const covered = (t: number) => {
    const p = at(t);
    const top = topAt({ x: p[0], y: p[1] });
    return top !== null && top > p[2] + COVER_TOL;
  };
  // Sample the middle of each slice so the line's own end points (shared
  // with the faces that meet there) do not decide.
  const flags = Array.from({ length: SAMPLES }, (_, i) => covered((i + 0.5) / SAMPLES));
  const out: LinePiece[] = [];
  let start = 0;
  for (let i = 1; i <= SAMPLES; i++) {
    if (i === SAMPLES || flags[i] !== flags[start]) {
      out.push({ a: at(start / SAMPLES), b: at(i / SAMPLES), hidden: flags[start] });
      start = i;
    }
  }
  return out;
}

/** The roof lines of a result split into seen and covered stretches, per part. */
export function coveredPieces(result: CalcResult): { kind: string; pieces: LinePiece[] }[][] {
  const many = result.parts.length > 1;
  const topAt = (p: Point2) => hostHeightAt(result, p);
  return result.parts.map((rp) =>
    rp.geometry.lines.map((l) => ({
      kind: l.kind,
      // One roof covers nothing of itself: skip the sampling.
      pieces: many ? splitByCover(l.a, l.b, topAt) : [{ a: l.a, b: l.b, hidden: false }],
    })),
  );
}

function pointIn(poly: Point2[], p: Point2): boolean {
  let c = false;
  for (let i = 0, j = poly.length - 1; i < poly.length; j = i++) {
    const [pi, pj] = [poly[i], poly[j]];
    if (pi.y > p.y !== pj.y > p.y && p.x < ((pj.x - pi.x) * (p.y - pi.y)) / (pj.y - pi.y) + pi.x) c = !c;
  }
  return c;
}

/** Shortest distance between segments ab and cd in plan. */
function segDist(a: Point2, b: Point2, c: Point2, d: Point2): number {
  const cross = (o: Point2, p: Point2, q: Point2) => (p.x - o.x) * (q.y - o.y) - (p.y - o.y) * (q.x - o.x);
  const d1 = cross(c, d, a);
  const d2 = cross(c, d, b);
  const d3 = cross(a, b, c);
  const d4 = cross(a, b, d);
  if (((d1 > 0 && d2 < 0) || (d1 < 0 && d2 > 0)) && ((d3 > 0 && d4 < 0) || (d3 < 0 && d4 > 0))) return 0;
  const pt = (p: Point2, s: Point2, e: Point2) => {
    const L = (e.x - s.x) ** 2 + (e.y - s.y) ** 2;
    const t = L ? Math.max(0, Math.min(1, ((p.x - s.x) * (e.x - s.x) + (p.y - s.y) * (e.y - s.y)) / L)) : 0;
    return Math.hypot(p.x - (s.x + t * (e.x - s.x)), p.y - (s.y + t * (e.y - s.y)));
  };
  return Math.min(pt(a, c, d), pt(b, c, d), pt(c, a, b), pt(d, a, b));
}

/**
 * For each part, the other parts its roof meets: footprints that overlap or
 * come closer than the two overhangs, so the eaves reach each other and the
 * calculation joins the roofs. O(n² · walls²).
 */
export function joinedParts(parts: RoofPart[]): number[][] {
  const pts = parts.map((p) => p.edges.map((e) => e.start));
  const ov = parts.map((p) => p.edges.reduce((m, e) => Math.max(m, e.overhang), 0));
  const meets = (i: number, j: number) => {
    const [A, B] = [pts[i], pts[j]];
    if (A.length < 3 || B.length < 3) return false;
    if (A.some((p) => pointIn(B, p)) || B.some((p) => pointIn(A, p))) return true;
    const reach = ov[i] + ov[j] + 1e-6;
    for (let a = 0; a < A.length; a++) {
      for (let b = 0; b < B.length; b++) {
        if (segDist(A[a], A[(a + 1) % A.length], B[b], B[(b + 1) % B.length]) <= reach) return true;
      }
    }
    return false;
  };
  return parts.map((_, i) => parts.flatMap((_, j) => (j !== i && meets(i, j) ? [j] : [])));
}
