// Where the plan writes a roof face's pitch and area: at the face's centre,
// turned along its eave (parallel to the face's contour lines) so it reads
// like the face labels of the reference program. Display only: pitch and
// area come from the core.

import type { RoofFace } from "./types";

export interface FaceTag {
  /** Plan position of the label (m). */
  x: number;
  y: number;
  /** Text direction in plan, degrees counter-clockwise from +x, in (-90, 90]. */
  angle_deg: number;
  pitch_deg: number;
  area: number;
  /** Extent of the face along and across the label direction (m). */
  along: number;
  across: number;
}

/** Area centroid of a plan polygon; the vertex mean when it has no area. */
export function centroid(poly: readonly (readonly number[])[]): [number, number] {
  let a2 = 0;
  let cx = 0;
  let cy = 0;
  for (let i = 0; i < poly.length; i++) {
    const p = poly[i];
    const q = poly[(i + 1) % poly.length];
    const c = p[0] * q[1] - q[0] * p[1];
    a2 += c;
    cx += (p[0] + q[0]) * c;
    cy += (p[1] + q[1]) * c;
  }
  if (Math.abs(a2) < 1e-9) {
    const n = poly.length || 1;
    return [poly.reduce((s, v) => s + v[0], 0) / n, poly.reduce((s, v) => s + v[1], 0) / n];
  }
  return [cx / (3 * a2), cy / (3 * a2)];
}

/** Direction of the face's horizontal lines in plan (degrees, in (-90, 90]),
 * from its Newell normal; 0 for a flat or degenerate face. */
export function contourAngle(poly: readonly (readonly number[])[]): number {
  let nx = 0;
  let ny = 0;
  for (let i = 0; i < poly.length; i++) {
    const p = poly[i];
    const q = poly[(i + 1) % poly.length];
    const pz = p[2] ?? 0;
    const qz = q[2] ?? 0;
    nx += (p[1] - q[1]) * (pz + qz);
    ny += (pz - qz) * (p[0] + q[0]);
  }
  if (Math.hypot(nx, ny) < 1e-9) return 0;
  // The contour runs perpendicular to the fall line (nx, ny).
  let deg = (Math.atan2(nx, -ny) * 180) / Math.PI;
  if (deg > 90) deg -= 180;
  if (deg <= -90) deg += 180;
  return Math.abs(deg) < 1e-9 ? 0 : deg;
}

/** One label per face with an area. */
export function faceTags(faces: readonly RoofFace[]): FaceTag[] {
  return faces
    .filter((f) => f.polygon.length >= 3 && f.area > 0)
    .map((f) => {
      const [x, y] = centroid(f.polygon);
      const angle_deg = contourAngle(f.polygon);
      const r = (angle_deg * Math.PI) / 180;
      const ux = Math.cos(r);
      const uy = Math.sin(r);
      let a0 = Infinity;
      let a1 = -Infinity;
      let b0 = Infinity;
      let b1 = -Infinity;
      for (const p of f.polygon) {
        const a = p[0] * ux + p[1] * uy;
        const b = -p[0] * uy + p[1] * ux;
        a0 = Math.min(a0, a);
        a1 = Math.max(a1, a);
        b0 = Math.min(b0, b);
        b1 = Math.max(b1, b);
      }
      return { x, y, angle_deg, pitch_deg: f.pitch_deg, area: f.area, along: a1 - a0, across: b1 - b0 };
    });
}
