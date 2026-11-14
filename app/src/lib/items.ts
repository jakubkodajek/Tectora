// Single objects on the roof (dormers, chimneys, roof windows, roof
// openings, hand-drawn lines and faces) as the plan selects, drags,
// rotates and duplicates them. Same rules as tectora-core
// `edit::transform::{transform_items, follow_roof}` (MCP `transform_items`).

import type { CalcResult, Placement, Point2, Project } from "./types";
import { mapChimney, mapDormer, mapManualFace, mapManualLine, mapWindow, power, rotation, translation, type PlanMap } from "./transform";
import { hostHeightAt } from "./dormers";

export type ItemKind = "dormer" | "chimney" | "window" | "hole" | "line" | "face";
export interface ItemRef {
  kind: ItemKind;
  index: number;
}

/** The editor's kind names against the file's / MCP's ones. */
export const MCP_KIND: Record<ItemKind, string> = {
  dormer: "dormer",
  chimney: "chimney",
  window: "window",
  hole: "roof_opening",
  line: "manual_line",
  face: "manual_face",
};

/** An object's distances on its roof face from the calculation, if it has any (lines and faces have none). */
export function placementOf(result: CalcResult | null, it: ItemRef | null): Placement | null {
  if (!result?.placements || !it) return null;
  const list =
    it.kind === "dormer" ? result.placements.dormers
    : it.kind === "chimney" ? result.placements.chimneys
    : it.kind === "window" ? result.placements.windows
    : it.kind === "hole" ? result.placements.roof_openings
    : null;
  return list?.[it.index] ?? null;
}

const r3 = (v: number) => Math.round(v * 1000) / 1000;
const avg = (pts: Point2[]): Point2 => ({
  x: r3(pts.reduce((s, p) => s + p.x, 0) / Math.max(pts.length, 1)),
  y: r3(pts.reduce((s, p) => s + p.y, 0) / Math.max(pts.length, 1)),
});

/** The list an item lives in (created when missing, so it can be appended to). */
function list(project: Project, kind: ItemKind): unknown[] {
  switch (kind) {
    case "dormer":
      return project.dormers;
    case "chimney":
      return project.chimneys;
    case "window":
      return (project.windows ??= []);
    case "hole":
      return (project.roof_openings ??= []);
    case "line":
      return (project.manual_lines ??= []);
    case "face":
      return (project.manual_faces ??= []);
  }
}

export function itemExists(project: Project, it: ItemRef): boolean {
  const n = { dormer: project.dormers.length, chimney: project.chimneys.length, window: project.windows?.length ?? 0, hole: project.roof_openings?.length ?? 0, line: project.manual_lines?.length ?? 0, face: project.manual_faces?.length ?? 0 }[it.kind];
  return it.index >= 0 && it.index < n;
}

/** Pivot of an item: centre of a chimney, opening or face, middle of a
 *  line or of a dormer's front, a window's anchor corner. */
export function itemCentre(project: Project, it: ItemRef): Point2 | null {
  if (!itemExists(project, it)) return null;
  switch (it.kind) {
    case "dormer": {
      const d = project.dormers[it.index];
      const s = d.edges[0]?.start ?? d.end;
      return avg([s, d.end]);
    }
    case "chimney":
      return avg(project.chimneys[it.index].corners);
    case "hole":
      return avg(project.roof_openings![it.index].corners);
    case "window": {
      const w = project.windows![it.index];
      return { x: w.x, y: w.y };
    }
    case "line": {
      const l = project.manual_lines![it.index];
      return avg([{ x: l.a[0], y: l.a[1] }, { x: l.b[0], y: l.b[1] }]);
    }
    case "face":
      return avg(project.manual_faces![it.index].points);
  }
}

function mapItem(project: Project, it: ItemRef, m: PlanMap): unknown {
  switch (it.kind) {
    case "dormer":
      return mapDormer(project.dormers[it.index], m);
    case "chimney":
      return mapChimney(project.chimneys[it.index], m);
    case "hole":
      return mapChimney(project.roof_openings![it.index], m);
    case "window":
      return mapWindow(project.windows![it.index], m);
    case "line":
      return mapManualLine(project.manual_lines![it.index], m);
    case "face":
      return mapManualFace(project.manual_faces![it.index], m);
  }
}

/**
 * Applies `m` to one item in place, or with `copies` > 0 appends that many
 * copies (the k-th mapped k times). Heights stay; see `followRoof`.
 * Returns the moved or new items.
 */
export function transformItem(project: Project, it: ItemRef, m: PlanMap, copies = 0): ItemRef[] {
  if (!itemExists(project, it)) return [];
  const l = list(project, it.kind);
  if (copies === 0) {
    l[it.index] = mapItem(project, it, m);
    return [it];
  }
  const out: ItemRef[] = [];
  for (let k = 1; k <= copies; k++) {
    l.push(mapItem(project, it, power(m, k)));
    out.push({ kind: it.kind, index: l.length - 1 });
  }
  return out;
}

export const moveItem = (project: Project, it: ItemRef, dx: number, dy: number) => transformItem(project, it, translation(dx, dy));

/** Turns an item about its own centre. */
export function rotateItem(project: Project, it: ItemRef, deg: number, copies = 0): ItemRef[] {
  const c = itemCentre(project, it);
  return c ? transformItem(project, it, rotation(c, deg), copies) : [];
}

/**
 * Keeps an item moved by `to - from` at the same height above the roof:
 * a chimney's top, a dormer's eave and a hand-drawn line's ends rise or
 * fall with the host roof (as tectora-core `follow_roof`). `result` is the
 * roof from before the move.
 */
export function followRoof(project: Project, it: ItemRef, from: Point2, to: Point2, result: CalcResult | null) {
  if (!result || !itemExists(project, it)) return;
  const delta = (a: Point2, b: Point2) => {
    const [za, zb] = [hostHeightAt(result, a), hostHeightAt(result, b)];
    return za === null || zb === null ? 0 : zb - za;
  };
  const d = delta(from, to);
  if (it.kind === "chimney") {
    const c = project.chimneys[it.index];
    c.height = r3(c.height + d);
  } else if (it.kind === "dormer") {
    for (const e of project.dormers[it.index].edges) e.eave_height = r3(e.eave_height + d);
  } else if (it.kind === "line") {
    const l = project.manual_lines![it.index];
    const off = { x: to.x - from.x, y: to.y - from.y };
    for (const q of [l.a, l.b]) {
      const was = { x: q[0] - off.x, y: q[1] - off.y };
      q[2] = r3(q[2] + delta(was, { x: q[0], y: q[1] }));
    }
  }
}

/** Removes an item. */
export function deleteItem(project: Project, it: ItemRef) {
  if (itemExists(project, it)) list(project, it.kind).splice(it.index, 1);
}

/** What a press in the plan or the 3D view hit. */
export type PlanTarget =
  | { type: "item"; item: ItemRef }
  | { type: "edge"; part: number; edge: number }
  | { type: "vertex"; part: number; vertex: number }
  | { type: "part"; part: number }
  | { type: "canvas" };
