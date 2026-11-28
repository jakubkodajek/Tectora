// Where the plan view looks: fitting the drawing, keeping it still when the
// bar below the plan changes height, and following a footprint being drawn.
// Pure (no Svelte), so the rules are unit-tested.

import { pushAll } from "./extent";
import { fitScale } from "./grid";

export interface Pt {
  x: number;
  y: number;
}

/** The plan's view transform: screen = (world - center) * scale + size / 2, y flipped. */
export interface View {
  scale: number;
  center: Pt;
}

export interface Box {
  minX: number;
  minY: number;
  maxX: number;
  maxY: number;
}

/**
 * Room kept around the drawing for the labels written outside the walls
 * (wall number and length, about 50 px wide), px per side (UI-010: the side
 * walls' labels were cut at the plan edge at 1024 px).
 */
export const LABEL_PAD = 56;

/** The bounding box of `pts`, or null for none / any coordinate that is not finite. */
export function boxOf(pts: Pt[]): Box | null {
  if (!pts.length) return null;
  let [minX, minY, maxX, maxY] = [Infinity, Infinity, -Infinity, -Infinity];
  for (const p of pts) {
    if (!Number.isFinite(p.x) || !Number.isFinite(p.y)) return null;
    minX = Math.min(minX, p.x);
    maxX = Math.max(maxX, p.x);
    minY = Math.min(minY, p.y);
    maxY = Math.max(maxY, p.y);
  }
  return { minX, minY, maxX, maxY };
}

/** What of a job and its calculated roof lies in the plan (structural: only the coordinates). */
export interface PlanContent {
  parts: { edges: { start: Pt }[] }[];
  manual_faces?: { points: Pt[] }[] | null;
  manual_lines?: { a: number[]; b: number[] }[] | null;
  towers?: { center: Pt; width: number; depth?: number | null }[] | null;
}

/**
 * Every point the plan should show: the walls, the hand-drawn faces and
 * lines (a face beside the house, or a job of faces only, is part of the
 * roof: UX-301, TEST-305), the eaves of the calculated roof (`faces`,
 * polygons of [x, y, …]; their labels lie outside the walls) and a
 * footprint being drawn.
 */
export function contentPoints(project: PlanContent, faces: readonly (readonly number[])[][] = [], drawing: readonly Pt[] | null = null): Pt[] {
  const pts: Pt[] = project.parts.flatMap((p) => p.edges.map((e) => e.start));
  for (const f of project.manual_faces ?? []) pushAll(pts, f.points);
  for (const l of project.manual_lines ?? []) pts.push({ x: l.a[0], y: l.a[1] }, { x: l.b[0], y: l.b[1] });
  // A tower's footprint, whichever way it is turned (it may stand beside the buildings).
  for (const tw of project.towers ?? []) {
    const r = Math.max(tw.width, tw.depth ?? 0) * 0.75;
    if (Number.isFinite(r) && Number.isFinite(tw.center.x) && Number.isFinite(tw.center.y)) pts.push({ x: tw.center.x - r, y: tw.center.y - r }, { x: tw.center.x + r, y: tw.center.y + r });
  }
  for (const polygon of faces) for (const q of polygon) pts.push({ x: q[0], y: q[1] });
  if (drawing) pushAll(pts, drawing);
  return pts;
}

/**
 * The view that shows all of `box` with the usual quarter margin and at least
 * `LABEL_PAD` px on every side for the wall labels; null when the view has no room.
 */
export function fitView(box: Box, width: number, height: number): View | null {
  const w = box.maxX - box.minX || 1;
  const h = box.maxY - box.minY || 1;
  const base = fitScale(width, height, w, h);
  if (base === null) return null;
  const padded = Math.min((width - 2 * LABEL_PAD) / w, (height - 2 * LABEL_PAD) / h);
  // A tiny view gets no padding rather than no plan.
  const scale = padded > 0 ? Math.min(base, padded) : base;
  return { scale, center: { x: (box.minX + box.maxX) / 2, y: (box.minY + box.maxY) / 2 } };
}

/** Whether the screen box of `box` lies within the view, `pad` px in from every edge. */
export function boxInView(box: Box, view: View, width: number, height: number, pad = 0): boolean {
  const sx = (x: number) => (x - view.center.x) * view.scale + width / 2;
  const sy = (y: number) => -(y - view.center.y) * view.scale + height / 2;
  return sx(box.minX) >= pad && sx(box.maxX) <= width - pad && sy(box.maxY) >= pad && sy(box.minY) <= height - pad;
}

/**
 * The view after the plan changed size from `from` to `to` (px), or "fit" to
 * fit the drawing again. When only the height changed (the bar below the plan
 * grew or shrank, e.g. its "More" row on selecting a wall) the drawing stays
 * where it is on screen, so the second press of a double click lands on the
 * same wall (UI-011); it is fitted again only when it would leave the view.
 */
export function resizedView(view: View, from: { width: number; height: number }, to: { width: number; height: number }, box: Box | null): View | "fit" {
  if (from.width !== to.width || !box) return "fit";
  // Keep the top edge: sy(y) unchanged for every y.
  const kept = { scale: view.scale, center: { x: view.center.x, y: view.center.y + (from.height - to.height) / (2 * view.scale) } };
  return boxInView(box, kept, to.width, to.height) ? kept : "fit";
}

/**
 * Whether a footprint being drawn must be brought back into view: its newest
 * corner lies outside the plan or within `pad` px of its edge (UX-001: typing
 * wall lengths ran the drawing out of view).
 */
export function draftLeavesView(draft: Pt[], view: View, width: number, height: number, pad = 24): boolean {
  const last = draft[draft.length - 1];
  if (!last || !Number.isFinite(last.x) || !Number.isFinite(last.y)) return false;
  return !boxInView({ minX: last.x, maxX: last.x, minY: last.y, maxY: last.y }, view, width, height, pad);
}
