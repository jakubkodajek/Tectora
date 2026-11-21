// Switching a wall between eave and gable, one rule for the wall bar's
// Eave / Gable buttons and the plan's context menu (UI-007).

import type { EdgeDef } from "./types";
import { isGable, isMarker } from "./types";

/**
 * The pitch each wall had before it became a gable, so Eave brings back
 * that wall's own pitch (not the last one switched). Keyed by part and the
 * wall's start corner, which survive undo and reloads; kept for the session.
 */
const remembered = new Map<string, number>();
const wallKey = (part: number, e: { start: { x: number; y: number } }) => `${part}:${e.start.x},${e.start.y}`;

/** Remembers the pitch of wall `e` of `part` before it is made a gable. */
export function rememberEavePitch(part: number, e: EdgeDef): void {
  remembered.set(wallKey(part, e), e.pitch_deg);
}

/**
 * The pitch for a gable turned back into an eave: its own from before,
 * else the nearest eave wall's in this part (the previous one first), else
 * the roof default (tectora-core `edit::eave_pitch`, parity-tested; MCP
 * `update_edges` with `gable: false` uses the same rule).
 */
export function eavePitch(edges: EdgeDef[], edge: number, own: number | null, preset: number): number {
  if (own !== null && Number.isFinite(own)) return own;
  const n = edges.length;
  for (let d = 1; d <= Math.floor(n / 2); d++) {
    for (const k of [edge - d, edge + d]) {
      const e = edges[((k % n) + n) % n];
      if (!isGable(e) && !isMarker(e)) return e.pitch_deg;
    }
  }
  return preset;
}

/** [`eavePitch`] for wall `edge` of `part`, with the pitch it had before when remembered. */
export function eavePitchFor(part: number, edges: EdgeDef[], edge: number, preset: number): number {
  const e = edges[edge];
  return eavePitch(edges, edge, e ? (remembered.get(wallKey(part, e)) ?? null) : null, preset);
}
