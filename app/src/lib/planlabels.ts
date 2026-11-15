// Which plan labels (wall dimensions, face pitch / area) are drawn when they
// would overlap, e.g. two buildings drawn into each other (UX-012). Display
// only; the figures themselves come from the core.

/** An axis-aligned screen box, px. */
export interface Box {
  x0: number;
  y0: number;
  x1: number;
  y1: number;
}

/** The screen box of a `w` × `h` label centred at (cx, cy) and turned by `deg` degrees. */
export function rotatedBox(cx: number, cy: number, w: number, h: number, deg: number): Box {
  const r = (deg * Math.PI) / 180;
  const c = Math.abs(Math.cos(r));
  const s = Math.abs(Math.sin(r));
  const hw = (w * c + h * s) / 2;
  const hh = (w * s + h * c) / 2;
  return { x0: cx - hw, y0: cy - hh, x1: cx + hw, y1: cy + hh };
}

/** True when the boxes share more than `gap` px in both directions. */
export function overlaps(a: Box, b: Box, gap = 1): boolean {
  return Math.min(a.x1, b.x1) - Math.max(a.x0, b.x0) > gap && Math.min(a.y1, b.y1) - Math.max(a.y0, b.y0) > gap;
}

/**
 * Greedy placement in the given order (most important first): a label is
 * kept when it overlaps none of the labels kept before it. O(n²), n is the
 * number of labels on screen (walls plus faces, a few hundred at most).
 */
export function placeLabels(boxes: (Box | null)[]): boolean[] {
  return chooseLabels(boxes.map((b) => [b])).map((j) => j === 0);
}

/** A screen segment [x1, y1, x2, y2], px. */
export type Seg = [number, number, number, number];

/**
 * True when the segment runs through the box shrunk by `inset` px on every
 * side (a line that only grazes a label's estimated edge does not count).
 * Liang-Barsky clipping.
 */
export function segCrossesBox(s: Seg, b: Box, inset = 3): boolean {
  const [x0, y0, x1, y1] = [b.x0 + inset, b.y0 + inset, b.x1 - inset, b.y1 - inset];
  if (!(x1 > x0 && y1 > y0)) return false;
  const dx = s[2] - s[0];
  const dy = s[3] - s[1];
  let t0 = 0;
  let t1 = 1;
  for (const [p, q] of [
    [-dx, s[0] - x0],
    [dx, x1 - s[0]],
    [-dy, s[1] - y0],
    [dy, y1 - s[1]],
  ]) {
    if (p === 0) {
      if (q < 0) return false;
    } else {
      const r = q / p;
      if (p < 0) t0 = Math.max(t0, r);
      else t1 = Math.min(t1, r);
      if (t0 > t1) return false;
    }
  }
  return true;
}

/**
 * Greedy placement with fallbacks: each label has candidate boxes in order
 * of preference (a wall length outside its building, then inside), null for
 * a candidate that may not be used; the first candidate that overlaps no
 * label kept before is taken. Returns the chosen candidate per label, or -1
 * when none fits (the label is not drawn). O(n² · candidates).
 *
 * `linesOf(i)`: the lines label `i` should not sit on (roof lines inside
 * the roof, walls): the first candidate clear of both the kept labels and
 * these lines wins; when every candidate is on a line, the one on the
 * fewest lines (the first of those) is taken (sweep 2026-11-15: a shared
 * wall's length on the ridge).
 */
export function chooseLabels(candidates: (Box | null)[][], linesOf: (i: number) => Seg[] = () => []): number[] {
  const kept: Box[] = [];
  const free = (b: Box | null): b is Box => !!b && !kept.some((k) => overlaps(k, b));
  return candidates.map((cands, i) => {
    const lines = linesOf(i);
    let j = -1;
    let least = Infinity;
    for (let c = 0; c < cands.length && least > 0; c++) {
      const b = cands[c];
      if (!free(b)) continue;
      const n = lines.reduce((m, l) => m + (segCrossesBox(l, b) ? 1 : 0), 0);
      if (n < least) [j, least] = [c, n];
    }
    if (j >= 0) kept.push(cands[j]!);
    return j;
  });
}

/** True when `p` lies inside the closed polygon `poly` (even-odd rule). */
export function insidePolygon(p: { x: number; y: number }, poly: { x: number; y: number }[]): boolean {
  let inside = false;
  for (let i = 0, j = poly.length - 1; i < poly.length; j = i++) {
    const a = poly[i];
    const b = poly[j];
    if (a.y > p.y !== b.y > p.y && p.x < ((b.x - a.x) * (p.y - a.y)) / (b.y - a.y) + a.x) inside = !inside;
  }
  return inside;
}

/**
 * A wall's number in the plan: "3" with one building, "2.3" (building 2,
 * wall 3) when there are several, so "wall 3" is never ambiguous (UX-018).
 */
export function wallNumber(part: number, edge: number, parts: number): string {
  return parts > 1 ? `${part + 1}.${edge + 1}` : `${edge + 1}`;
}

/** Height over which a face counts as covered by another roof, m [no source: a few cm, as for covered lines]. */
const COVER_TOL = 0.05;

/**
 * True when a face's label point lies under a higher roof face (another
 * building's roof over this one): its label would be written over a roof
 * that is not the one it describes. `topAt` is the highest roof face over a
 * plan point, `z` the face's own height there.
 */
export function underHigherRoof(z: number | null, topAt: number | null): boolean {
  return z !== null && topAt !== null && topAt > z + COVER_TOL;
}
