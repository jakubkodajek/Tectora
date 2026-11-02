// Moving, rotating, mirroring and copying a roof part with everything that
// sits on it (DACAD: Bearbeiten › Verschieben, Drehen, Spiegeln, Elemente
// kopieren, Kopieren-Drehen, Kopieren-Spiegeln). Plain plan geometry: the
// roof is recalculated afterwards, so no roof rule is involved.

import type { Chimney, Dormer, EdgeDef, ManualFace, ManualLine, Point2, Project, RoofPart, RoofWindow } from "./types";

export type TransformMode = "move" | "rotate" | "mirror" | "wall";
/** What the transform dialog asks for: a map of the part (with copies) or a wall shift. */
export type TransformOp =
  | { kind: "map"; map: PlanMap; copies: number }
  | { kind: "wall"; edge: number; dist: number };

/** An affine plan map; `flips` tells whether it reverses orientation. */
export interface PlanMap {
  apply: (p: Point2) => Point2;
  /** Maps a direction angle (radians). */
  angle: (a: number) => number;
  flips: boolean;
}

/** Deep copy that also works on reactive (proxied) project state. */
const clone = <T>(v: T): T => JSON.parse(JSON.stringify(v));

const r3 = (v: number) => Math.round(v * 1000) / 1000;
const pt = (p: Point2): Point2 => ({ x: r3(p.x), y: r3(p.y) });

export function translation(dx: number, dy: number): PlanMap {
  return { apply: (p) => pt({ x: p.x + dx, y: p.y + dy }), angle: (a) => a, flips: false };
}

/** Rotation by `deg` degrees (counter-clockwise) about `c`. */
export function rotation(c: Point2, deg: number): PlanMap {
  const a = (deg * Math.PI) / 180;
  const [cos, sin] = [Math.cos(a), Math.sin(a)];
  return {
    apply: (p) => pt({ x: c.x + (p.x - c.x) * cos - (p.y - c.y) * sin, y: c.y + (p.x - c.x) * sin + (p.y - c.y) * cos }),
    angle: (t) => t + a,
    flips: false,
  };
}

/** Mirror at the line through `c` with direction `deg` degrees. */
export function mirror(c: Point2, deg: number): PlanMap {
  const t = (deg * Math.PI) / 180;
  const [cos2, sin2] = [Math.cos(2 * t), Math.sin(2 * t)];
  return {
    apply: (p) => {
      const [x, y] = [p.x - c.x, p.y - c.y];
      return pt({ x: c.x + x * cos2 + y * sin2, y: c.y + x * sin2 - y * cos2 });
    },
    angle: (a) => 2 * t - a,
    flips: true,
  };
}

/** `m` applied `k` times (for the k-th copy). */
export function power(m: PlanMap, k: number): PlanMap {
  let out: PlanMap = { apply: (p) => p, angle: (a) => a, flips: false };
  for (let i = 0; i < k; i++) {
    const prev = out;
    out = { apply: (p) => m.apply(prev.apply(p)), angle: (a) => m.angle(prev.angle(a)), flips: prev.flips !== m.flips };
  }
  return out;
}

const normAngle = (a: number) => {
  const t = 2 * Math.PI;
  return ((a % t) + t) % t;
};

/**
 * Maps a chain of walls: edge i runs from `pts[i]` to `pts[i + 1]`, where
 * `pts` are the edge starts followed by `end` (the first start for a closed
 * outline). A map that flips orientation also reverses the chain, so the
 * roof stays on the same side of each wall; a direction marker then turns
 * by 180°, as the engine does when it reverses an outline.
 */
function mapChain(edges: EdgeDef[], end: Point2, m: PlanMap): { edges: EdgeDef[]; end: Point2 } {
  const mapEdge = (e: EdgeDef, start: Point2, turn: number): EdgeDef => ({
    ...clone(e),
    start: m.apply(start),
    ...(e.direction_rad != null ? { direction_rad: normAngle(m.angle(e.direction_rad) + turn) } : {}),
  });
  if (!m.flips) {
    return { edges: edges.map((e) => mapEdge(e, e.start, 0)), end: m.apply(end) };
  }
  const pts = [...edges.map((e) => e.start), end];
  const n = edges.length;
  const out: EdgeDef[] = [];
  for (let j = 0; j < n; j++) {
    const k = n - 1 - j;
    out.push(mapEdge(edges[k], pts[k + 1], Math.PI));
  }
  return { edges: out, end: m.apply(pts[0]) };
}

export function mapPart(part: RoofPart, m: PlanMap, name = part.name): RoofPart {
  const first = part.edges[0]?.start ?? { x: 0, y: 0 };
  const { edges } = mapChain(part.edges, first, m);
  return { ...clone(part), name, edges };
}

export function mapDormer(d: Dormer, m: PlanMap): Dormer {
  const { edges, end } = mapChain(d.edges, d.end, m);
  return { ...clone(d), edges, end };
}

export function mapChimney(c: Chimney, m: PlanMap): Chimney {
  const corners = c.corners.map(m.apply);
  if (m.flips) corners.reverse();
  return { ...clone(c), corners };
}

/** A roof window is placed by its anchor point; the roof face it lands on gives its slope. */
export function mapWindow(w: RoofWindow, m: PlanMap): RoofWindow {
  const p = m.apply({ x: w.x, y: w.y });
  return { ...w, x: p.x, y: p.y };
}

export function mapManualLine(l: ManualLine, m: PlanMap): ManualLine {
  const a = m.apply({ x: l.a[0], y: l.a[1] });
  const b = m.apply({ x: l.b[0], y: l.b[1] });
  return { ...clone(l), a: [a.x, a.y, l.a[2]], b: [b.x, b.y, l.b[2]] };
}

export function mapManualFace(f: ManualFace, m: PlanMap): ManualFace {
  const points = f.points.map(m.apply);
  if (m.flips) points.reverse();
  return { ...clone(f), points };
}

/** `base (n)` with the smallest n >= 2 no part uses yet. */
function uniqueName(project: Project, base: string): string {
  const root = base.replace(/ \(\d+\)$/, "");
  const used = new Set(project.parts.map((p) => p.name));
  for (let n = 2; ; n++) if (!used.has(`${root} (${n})`)) return `${root} (${n})`;
}

/** Even-odd point-in-polygon test on the part's wall outline. */
export function insideOutline(part: RoofPart, p: Point2): boolean {
  const o = part.edges.map((e) => e.start);
  let inside = false;
  for (let i = 0, j = o.length - 1; i < o.length; j = i++) {
    const [a, b] = [o[i], o[j]];
    if (a.y > p.y !== b.y > p.y && p.x < ((b.x - a.x) * (p.y - a.y)) / (b.y - a.y) + a.x) inside = !inside;
  }
  return inside;
}

/** Centre of a part's wall outline (vertex average), the default pivot. */
export function partCentre(part: RoofPart): Point2 {
  const n = Math.max(part.edges.length, 1);
  return pt({
    x: part.edges.reduce((s, e) => s + e.start.x, 0) / n,
    y: part.edges.reduce((s, e) => s + e.start.y, 0) / n,
  });
}

const centre = (pts: Point2[]): Point2 => ({
  x: pts.reduce((s, p) => s + p.x, 0) / Math.max(pts.length, 1),
  y: pts.reduce((s, p) => s + p.y, 0) / Math.max(pts.length, 1),
});

/**
 * Applies `m` to part `pi` and to the dormers, chimneys, openings, windows
 * and hand-drawn lines and faces lying on it. With `copies` > 0 the part
 * stays and that many copies are added, the k-th mapped k times (DACAD's
 * copy functions). Returns the indices of the new or moved parts.
 */
export function transformPart(project: Project, pi: number, m: PlanMap, copies = 0): number[] {
  const part = project.parts[pi];
  if (!part) return [];
  const on = (p: Point2) => insideOutline(part, p);
  const dormers = project.dormers.filter((d) => on(centre([...d.edges.map((e) => e.start), d.end])));
  const chimneys = project.chimneys.filter((c) => on(centre(c.corners)));
  const holes = (project.roof_openings ?? []).filter((c) => on(centre(c.corners)));
  const windows = (project.windows ?? []).filter((w) => on({ x: w.x, y: w.y }));
  const lines = (project.manual_lines ?? []).filter((l) => on({ x: (l.a[0] + l.b[0]) / 2, y: (l.a[1] + l.b[1]) / 2 }));
  const faces = (project.manual_faces ?? []).filter((f) => on(centre(f.points)));

  if (copies <= 0) {
    project.parts[pi] = mapPart(part, m);
    const swap = <T>(list: T[] | undefined, mine: T[], f: (x: T, m: PlanMap) => T) => {
      if (!list) return;
      for (let i = 0; i < list.length; i++) if (mine.includes(list[i])) list[i] = f(list[i], m);
    };
    swap(project.dormers, dormers, mapDormer);
    swap(project.chimneys, chimneys, mapChimney);
    swap(project.roof_openings, holes, mapChimney);
    swap(project.windows, windows, mapWindow);
    swap(project.manual_lines, lines, mapManualLine);
    swap(project.manual_faces, faces, mapManualFace);
    return [pi];
  }
  const added: number[] = [];
  for (let k = 1; k <= copies; k++) {
    const mk = power(m, k);
    project.parts.push(mapPart(part, mk, uniqueName(project, part.name)));
    added.push(project.parts.length - 1);
    project.dormers.push(...dormers.map((d) => mapDormer(d, mk)));
    project.chimneys.push(...chimneys.map((c) => mapChimney(c, mk)));
    if (holes.length) (project.roof_openings ??= []).push(...holes.map((c) => mapChimney(c, mk)));
    if (windows.length) (project.windows ??= []).push(...windows.map((w) => mapWindow(w, mk)));
    if (lines.length) (project.manual_lines ??= []).push(...lines.map((l) => mapManualLine(l, mk)));
    if (faces.length) (project.manual_faces ??= []).push(...faces.map((f) => mapManualFace(f, mk)));
  }
  return added;
}

/**
 * Moves wall `ei` of a part parallel to itself by `d` metres outward (the
 * roof side is to the left of the wall in a counter-clockwise outline, so
 * outward is to the right), its neighbours stretched to meet it (DACAD:
 * Parallele Linie / Wände ändern). Returns false if a neighbour runs
 * parallel to the wall, where no corner can be found.
 */
export function offsetWall(part: RoofPart, ei: number, d: number): boolean {
  const n = part.edges.length;
  const P = (i: number) => part.edges[((i % n) + n) % n].start;
  const a = P(ei);
  const b = P(ei + 1);
  const len = Math.hypot(b.x - a.x, b.y - a.y);
  if (len < 1e-9) return false;
  // Outward normal: right of the wall direction for a CCW outline.
  const area = part.edges.reduce((s, e, i) => s + e.start.x * P(i + 1).y - P(i + 1).x * e.start.y, 0);
  const sgn = area >= 0 ? 1 : -1;
  const u = { x: (b.x - a.x) / len, y: (b.y - a.y) / len };
  const nrm = { x: sgn * u.y, y: -sgn * u.x };
  const a2 = { x: a.x + nrm.x * d, y: a.y + nrm.y * d };
  const b2 = { x: b.x + nrm.x * d, y: b.y + nrm.y * d };
  const meet = (p: Point2, q: Point2, s: Point2): Point2 | null => {
    // Line p->q (neighbour) with the shifted wall through s along u.
    const v = { x: q.x - p.x, y: q.y - p.y };
    const den = v.x * u.y - v.y * u.x;
    if (Math.abs(den) < 1e-9) return null;
    const t = ((s.x - p.x) * u.y - (s.y - p.y) * u.x) / den;
    return { x: p.x + v.x * t, y: p.y + v.y * t };
  };
  const na = meet(P(ei - 1), a, a2);
  const nb = meet(P(ei + 2), b, b2);
  if (!na || !nb) return false;
  part.edges[ei].start = pt(na);
  part.edges[(ei + 1) % n].start = pt(nb);
  return true;
}

/** The corners of a regular polygon (DACAD: Vieleck-Eingabe), counter-clockwise from the first corner. */
export function regularPolygon(c: Point2, sides: number, side: number, rotationDeg = 0): Point2[] {
  const r = side / (2 * Math.sin(Math.PI / sides));
  // First wall horizontal at the bottom for rotation 0.
  const a0 = -Math.PI / 2 - Math.PI / sides + (rotationDeg * Math.PI) / 180;
  return Array.from({ length: sides }, (_, i) => {
    const a = a0 + (2 * Math.PI * i) / sides;
    return pt({ x: c.x + r * Math.cos(a), y: c.y + r * Math.sin(a) });
  });
}
