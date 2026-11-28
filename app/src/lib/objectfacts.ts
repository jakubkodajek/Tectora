// Read-only facts about a chimney, roof window or roof opening for its
// editor (OpeningBar) and the sketch of its roof face (PlaceSketch). Display
// only: every value the job stores goes through the core; these restate the
// core's results (Placement, the calculated faces) in the editor's words.

import type { CalcResult, Placement, Point2, RoofFace } from "./types";
import { hostHeightAt, inside } from "./dormers";
import { maxOf, minOf } from "./extent";

/** Plan area of a polygon (shoelace), m². */
export function planArea(k: readonly Point2[]): number {
  let a = 0;
  for (let i = 0; i < k.length; i++) {
    const p = k[i], q = k[(i + 1) % k.length];
    a += p.x * q.y - q.x * p.y;
  }
  return Math.abs(a) / 2;
}

/** A plan area laid on a face of `pitchDeg`: the area the roof loses (as the results panel deducts it). */
export const slopeArea = (plan: number, pitchDeg: number) => plan / Math.cos((pitchDeg * Math.PI) / 180);

/** A length along the slope seen in plan. */
export const planLength = (slope: number, pitchDeg: number) => slope * Math.cos((pitchDeg * Math.PI) / 180);

/**
 * How far a chimney's top stands above the roof on its upper and lower side:
 * the top minus the roof height under its corners, each taken 1 mm inside the
 * chimney (as the dormer's host height). Null when a corner is off the roof.
 */
export function aboveRoof(result: CalcResult | null, corners: readonly Point2[], top: number): { up: number; down: number } | null {
  const n = corners.length;
  if (n < 3 || !Number.isFinite(top)) return null;
  const c = { x: corners.reduce((s, p) => s + p.x, 0) / n, y: corners.reduce((s, p) => s + p.y, 0) / n };
  const zs: number[] = [];
  for (const p of corners) {
    const d = Math.hypot(c.x - p.x, c.y - p.y);
    const k = d > 1e-6 ? 1e-3 / d : 0;
    const z = hostHeightAt(result, { x: p.x + (c.x - p.x) * k, y: p.y + (c.y - p.y) * k });
    if (z === null || !Number.isFinite(z)) return null;
    zs.push(z);
  }
  return { up: top - maxOf(zs), down: top - minOf(zs) };
}

/**
 * The directions of a placement in plan: `up` the slope (the core's
 * `eave_line` runs from the eave up to the object) and `right` along the
 * eave as seen looking up from the eave (`left_line` and `right_line` run
 * that way). edit::position builds `right` as `up` turned clockwise.
 */
export function placeFrame(pl: Placement): { up: Point2; right: Point2 } {
  const unit = (a: Point2, b: Point2): Point2 | null => {
    const dx = b.x - a.x, dy = b.y - a.y;
    const l = Math.hypot(dx, dy);
    return l > 1e-3 && Number.isFinite(l) ? { x: dx / l, y: dy / l } : null;
  };
  let up = unit(pl.eave_line[0], pl.eave_line[1]);
  if (!up) {
    const right = unit(pl.left_line[0], pl.left_line[1]) ?? unit(pl.right_line[0], pl.right_line[1]) ?? { x: 1, y: 0 };
    up = { x: -right.y, y: right.x };
  }
  return { up, right: { x: up.y, y: -up.x } };
}

/**
 * The faces of the wall a placement is on, and which of them holds the
 * object (the one the distances are measured on: its lowest edge is where
 * "from eave" starts; above a pitch break that is the break, not the eave).
 */
export function placeFaces(result: CalcResult | null, pl: Placement): { faces: RoofFace[]; host: number; hostIsEave: boolean } | null {
  const faces = (result?.parts[pl.part]?.geometry.faces ?? []).filter((f) => f.edge === pl.edge && f.polygon.length >= 3);
  if (!faces.length) return null;
  const { up } = placeFrame(pl);
  // Just above the object's lowest point, inside the face it stands on.
  const probe = { x: pl.eave_line[1].x + up.x * 1e-3, y: pl.eave_line[1].y + up.y * 1e-3 };
  let host = faces.findIndex((f) => inside(f.polygon, probe));
  if (host < 0) host = 0;
  const low = (f: RoofFace) => minOf(f.polygon, (q) => q[2]);
  const wallLow = minOf(faces, low);
  return { faces, host, hostIsEave: low(faces[host]) - wallLow < 1e-3 };
}
