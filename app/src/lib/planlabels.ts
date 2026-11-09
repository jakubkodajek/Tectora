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
  const kept: Box[] = [];
  return boxes.map((b) => {
    if (!b || kept.some((k) => overlaps(k, b))) return false;
    kept.push(b);
    return true;
  });
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
