// What the editor draws of a tower: the dialog's section sketch from the
// rings the core calculated (`tower_preview`), and a plain outline of the
// footprint for a tower the core did not calculate (its problems). The
// roof itself, its areas and lines come from the core only.

import { maxOf } from "./extent";
import type { Point2, Tower, TowerResult } from "./types";

/** One point of the section: half the tower's width across its x axis at a height. */
export interface SectionPoint {
  half: number;
  z: number;
}

/**
 * The tower's profile as the dialog sketches it: for every ring of every
 * tier (bottom to top), the farthest any of its points lies from the axis
 * along the tower's own x axis, with the ring's height. Display only.
 */
export function towerSection(tower: Pick<Tower, "center" | "rotation_deg">, result: TowerResult | null): SectionPoint[] {
  if (!result) return [];
  const a = ((tower.rotation_deg ?? 0) * Math.PI) / 180;
  const [c, s] = [Math.cos(a), Math.sin(a)];
  const out: SectionPoint[] = [];
  for (const tier of result.tiers) {
    for (const ring of tier.rings) {
      if (!ring.length) continue;
      let half = 0;
      for (const p of ring) half = Math.max(half, Math.abs((p[0] - tower.center.x) * c + (p[1] - tower.center.y) * s));
      const z = ring[0][2];
      const last = out[out.length - 1];
      // The top ring of a tier is the bottom ring of the next.
      if (last && Math.abs(last.half - half) < 1e-9 && Math.abs(last.z - z) < 1e-9) continue;
      out.push({ half, z });
    }
  }
  return out;
}

/**
 * A plain outline of the tower's footprint from its own fields (the corner
 * radius left out), for drawing a tower the core refused: a rectangle, a
 * regular polygon across the flats, or a circle as 32 points. Empty for
 * values that are no numbers.
 */
export function roughFootprint(tower: Tower): Point2[] {
  const w = tower.width;
  const d = tower.depth ?? w;
  if (![w, d, tower.center.x, tower.center.y].every(Number.isFinite) || w <= 0 || d <= 0) return [];
  const a = ((tower.rotation_deg ?? 0) * Math.PI) / 180;
  const place = (x: number, y: number): Point2 => ({
    x: tower.center.x + x * Math.cos(a) - y * Math.sin(a),
    y: tower.center.y + x * Math.sin(a) + y * Math.cos(a),
  });
  const shape = tower.shape ?? "rectangle";
  if (shape === "rectangle") return [place(-w / 2, -d / 2), place(w / 2, -d / 2), place(w / 2, d / 2), place(-w / 2, d / 2)];
  const n = shape === "circle" ? 32 : Math.min(64, Math.max(3, Math.round(tower.sides ?? 8)));
  const r = shape === "circle" ? w / 2 : w / 2 / Math.cos(Math.PI / n);
  return Array.from({ length: n }, (_, k) => {
    const t = (2 * Math.PI * k) / n - (shape === "circle" ? 0 : Math.PI / n);
    return place(r * Math.cos(t), r * Math.sin(t));
  });
}

/**
 * Where a new tower's roof starts: 3 m above the highest roof under its
 * footprint (the centre and the corners of a `width` square), or 3 m above
 * `fallback` (the eave) off the roof [no source: editor choice, a start to
 * edit in the dialog].
 */
export function defaultTowerBase(heightAt: (p: Point2) => number | null, centre: Point2, width: number, fallback: number): number {
  const h = width / 2;
  const zs = [centre, ...[[-h, -h], [h, -h], [h, h], [-h, h]].map(([x, y]) => ({ x: centre.x + x, y: centre.y + y }))]
    .map((p) => heightAt(p))
    .filter((z): z is number => z !== null && Number.isFinite(z));
  const top = zs.length ? maxOf(zs) : fallback;
  return Math.round((Math.max(0, top) + 3) * 100) / 100;
}
