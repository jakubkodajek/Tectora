// Lines and faces added by hand to the calculated roof (DACAD:
// Nachbearbeitung › Dachlinieneingabe and Neue Dachfläche). Measured in
// DACAD 2026-10-29: a line's ends are the roof points it snaps to, with
// their height, else at height 0; the default type is a dimension line; a
// line of a roof type counts in that type's totals; a new face asks for its
// pitch (30° by default). DACAD drops these when the roof is recalculated;
// Tectora keeps them in the project. Totals are added by the engine
// (tectora-core quantities::manual_quantities).
import type { CalcResult, ManualFace, ManualLine, Point2 } from "./types";

/** DACAD's line types for hand-drawn lines, in its list order. */
export const MANUAL_CODES = ["M", "T", "F", "G", "K", "P", "OL", "OR", "OS", "c", "S", "WV", "WH", "WS", "X"] as const;
/** Codes counted in the line totals (as tectora-core COUNTED_LINE_CODES). */
export const COUNTED = new Set(["T", "F", "G", "K", "OL", "OR", "OS", "P", "c", "S", "WV", "WH", "WS"]);

/** Plan colour key (PlanView's line colours) of a code. */
export const codeKind: Record<string, string> = {
  T: "eave",
  F: "ridge",
  G: "hip",
  K: "valley",
  P: "pult",
  OL: "verge",
  OR: "verge",
  OS: "verge",
  c: "knick",
  S: "gorge",
  WV: "abutment",
  WH: "abutment",
  WS: "abutment",
};

export type ManualSel = { kind: "line" | "face"; index: number } | null;

/** Roof points to snap to: the ends of the calculated lines and face corners. */
export function roofPointList(result: CalcResult | null): [number, number, number][] {
  const out: [number, number, number][] = [];
  const seen = new Set<string>();
  const add = (q: [number, number, number]) => {
    const k = q.map((v) => v.toFixed(3)).join(",");
    if (!seen.has(k)) {
      seen.add(k);
      out.push([q[0], q[1], q[2]]);
    }
  };
  for (const part of result?.parts ?? []) {
    for (const l of part.geometry.lines) {
      add(l.a);
      add(l.b);
    }
    for (const f of part.geometry.faces) f.polygon.forEach(add);
  }
  return out;
}

/** The roof point within `tol` of `p` in plan, else `p` at height 0 (as DACAD). */
export function snapToRoof(result: CalcResult | null, p: Point2, tol: number): [number, number, number] {
  let best: [number, number, number] | null = null;
  let bd = tol;
  for (const q of roofPointList(result)) {
    const d = Math.hypot(q[0] - p.x, q[1] - p.y);
    if (d <= bd) {
      bd = d;
      best = q;
    }
  }
  return best ?? [p.x, p.y, 0];
}

export function lineLength(l: ManualLine): number {
  if (l.length != null) return l.length;
  return Math.hypot(l.b[0] - l.a[0], l.b[1] - l.a[1], l.b[2] - l.a[2]);
}

/** Sloped area: plan area over the cosine of the pitch (as the engine). */
export function faceArea(f: ManualFace): number {
  const n = f.points.length;
  if (n < 3 || !(f.pitch_deg >= 0 && f.pitch_deg < 90)) return 0;
  let a = 0;
  for (let i = 0; i < n; i++) {
    const p = f.points[i];
    const q = f.points[(i + 1) % n];
    a += p.x * q.y - q.x * p.y;
  }
  return Math.abs(a) / 2 / Math.cos((f.pitch_deg * Math.PI) / 180);
}
