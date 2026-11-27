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
 * kept when it overlaps none of the labels kept before it.
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

/** Cell size of the screen grids below, px: about a label's width. */
const CELL = 128;

/** The grid cells a box covers, as "column,row" numbers. */
function cells(b: Box, visit: (key: number) => void) {
  const [c0, c1, r0, r1] = [Math.floor(b.x0 / CELL), Math.floor(b.x1 / CELL), Math.floor(b.y0 / CELL), Math.floor(b.y1 / CELL)];
  for (let c = c0; c <= c1; c++) for (let r = r0; r <= r1; r++) visit(c * 65536 + r);
}

/**
 * Screen segments looked up by box: `near(b)` gives every segment whose own
 * box meets `b` (each once, in the order given), `nearAt(b)` their indices.
 * A segment that crosses a box has a box that meets it, so testing only
 * these is the same as testing all. A plan of 200 buildings tested every
 * label against every wall and roof line at each zoom or pan step and
 * froze for seconds (UX-602).
 *
 * `within`: only segments that meet this box are kept (the visible part of
 * the plan, with a margin); a wall far outside the window is never asked for.
 */
export interface SegIndex {
  near(b: Box): Seg[];
  nearAt(b: Box): number[];
  /** Segments kept (inside `within`). */
  readonly size: number;
}

export function segIndex(segs: readonly Seg[], within?: Box): SegIndex {
  const grid = new Map<number, number[]>();
  const boxes: (Box | null)[] = [];
  let size = 0;
  segs.forEach((s, i) => {
    let b: Box = { x0: Math.min(s[0], s[2]), y0: Math.min(s[1], s[3]), x1: Math.max(s[0], s[2]), y1: Math.max(s[1], s[3]) };
    if (within) {
      if (b.x1 < within.x0 || b.x0 > within.x1 || b.y1 < within.y0 || b.y0 > within.y1) {
        boxes.push(null);
        return;
      }
      // Only the cells inside the window: a wall many screens long covers few of them.
      b = { x0: Math.max(b.x0, within.x0), y0: Math.max(b.y0, within.y0), x1: Math.min(b.x1, within.x1), y1: Math.min(b.y1, within.y1) };
    }
    boxes.push(b);
    size++;
    cells(b, (key) => {
      const cell = grid.get(key);
      if (cell) cell.push(i);
      else grid.set(key, [i]);
    });
  });
  const seen = new Uint32Array(segs.length);
  let query = 0;
  const nearAt = (b: Box): number[] => {
    query++;
    const out: number[] = [];
    cells(b, (key) => {
      for (const i of grid.get(key) ?? []) {
        if (seen[i] === query) continue;
        seen[i] = query;
        const o = boxes[i]!;
        if (o.x1 >= b.x0 && o.x0 <= b.x1 && o.y1 >= b.y0 && o.y0 <= b.y1) out.push(i);
      }
    });
    return out.sort((x, y) => x - y);
  };
  return { near: (b) => nearAt(b).map((i) => segs[i]), nearAt, size };
}

/** Work counted by `chooseLabels` for its guard test (UX-602). */
export interface LabelStats {
  /** Label boxes compared with a kept label. */
  overlaps: number;
  /** Lines tested against a label box. */
  lines: number;
}

/**
 * Greedy placement with fallbacks: each label has candidate boxes in order
 * of preference (a wall length outside its building, then inside), null for
 * a candidate that may not be used; the first candidate that overlaps no
 * label kept before is taken. Returns the chosen candidate per label, or -1
 * when none fits (the label is not drawn). Kept labels are looked up by a
 * screen grid, so a label is compared only with the labels near it.
 *
 * `linesOf(i)`: the lines label `i` should not sit on (roof lines inside
 * the roof, walls), as a list or a `segIndex`: the first candidate clear of
 * both the kept labels and these lines wins; when every candidate is on a
 * line, the one on the fewest lines (the first of those) is taken (sweep
 * 2026-11-15: a shared wall's length on the ridge).
 */
export function chooseLabels(candidates: (Box | null)[][], linesOf: (i: number) => readonly Seg[] | SegIndex = () => [], stats?: LabelStats): number[] {
  const kept = new Map<number, Box[]>();
  const free = (b: Box | null): b is Box => {
    if (!b) return false;
    let hit = false;
    cells(b, (key) => {
      if (hit) return;
      for (const k of kept.get(key) ?? []) {
        if (stats) stats.overlaps++;
        if (overlaps(k, b)) {
          hit = true;
          return;
        }
      }
    });
    return !hit;
  };
  return candidates.map((cands, i) => {
    const lines = linesOf(i);
    let j = -1;
    let least = Infinity;
    for (let c = 0; c < cands.length && least > 0; c++) {
      const b = cands[c];
      if (!free(b)) continue;
      const near = "near" in lines ? lines.near(b) : lines;
      if (stats) stats.lines += near.length;
      const n = near.reduce((m, l) => m + (segCrossesBox(l, b) ? 1 : 0), 0);
      if (n < least) [j, least] = [c, n];
    }
    if (j >= 0) {
      const b = cands[j]!;
      cells(b, (key) => {
        const cell = kept.get(key);
        if (cell) cell.push(b);
        else kept.set(key, [b]);
      });
    }
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

/** A place for a dimension's figure: its anchor point, how the text is anchored there, and its box. */
export interface DimSpot {
  x: number;
  y: number;
  anchor: "start" | "middle" | "end";
  box: Box;
}

/** Height of an 11 px dimension figure with its halo, px. */
const PLACE_DIM_H = 16;

/**
 * Where a placed object's dimension figure ("2,381") may be written, best
 * first: beside the middle of its dimension line (`g`, screen px), then on
 * the other side and off the middle. The plan's label pass takes the first
 * spot clear of the labels kept before, so the eave, left and right figures
 * and the face labels never pile up in a small plan (UI-209). `eave`: the
 * distance from the eave, written to the right of its line.
 */
export function placeDimSpots(
  g: { mx: number; my: number; x1: number; y1: number; x2: number; y2: number },
  text: string,
  eave: boolean,
): DimSpot[] {
  const w = text.length * 11 * 0.62 + 8;
  const h = PLACE_DIM_H;
  const at = (x: number, y: number, anchor: DimSpot["anchor"]): DimSpot => {
    const x0 = anchor === "start" ? x : anchor === "end" ? x - w : x - w / 2;
    return { x, y, anchor, box: { x0, y0: y - h / 2, x1: x0 + w, y1: y + h / 2 } };
  };
  const q = (t: number) => ({ x: g.x1 + (g.x2 - g.x1) * t, y: g.y1 + (g.y2 - g.y1) * t });
  if (eave) {
    const [a, b] = [q(0.25), q(0.75)];
    return [at(g.mx + 6, g.my, "start"), at(g.mx - 6, g.my, "end"), at(a.x + 6, a.y, "start"), at(b.x + 6, b.y, "start"), at(a.x - 6, a.y, "end"), at(b.x - 6, b.y, "end")];
  }
  const [a, b] = [q(0.25), q(0.75)];
  return [at(g.mx, g.my - 6, "middle"), at(g.mx, g.my + 10, "middle"), at(a.x, a.y - 6, "middle"), at(b.x, b.y - 6, "middle"), at(a.x, a.y + 10, "middle"), at(b.x, b.y + 10, "middle")];
}
