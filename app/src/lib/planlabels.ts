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

/**
 * Greedy placement with fallbacks: each label has candidate boxes in order
 * of preference (a wall length outside its building, then inside), null for
 * a candidate that may not be used; the first candidate that overlaps no
 * label kept before is taken. Returns the chosen candidate per label, or -1
 * when none fits (the label is not drawn). O(n² · candidates).
 */
export function chooseLabels(candidates: (Box | null)[][]): number[] {
  const kept: Box[] = [];
  return candidates.map((cands) => {
    for (let j = 0; j < cands.length; j++) {
      const b = cands[j];
      if (b && !kept.some((k) => overlaps(k, b))) {
        kept.push(b);
        return j;
      }
    }
    return -1;
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
