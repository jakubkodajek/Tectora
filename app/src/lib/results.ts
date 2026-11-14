// What the Results panel says about a calculation besides its figures: the
// warnings of each part and the parts each roof is joined with. Both come
// from the core (`check.rs`, the Modellverschnitt's own join); nothing here
// guesses from the plan.

import { num } from "./format";
import { t } from "./i18n.svelte";
import { partLabel } from "./partname";
import type { CalcResult, RoofWarning } from "./types";

/** The translated sentence of one plausibility warning. */
export function warningText(w: RoofWarning): string {
  switch (w.kind) {
    case "merged_walls": {
      const [a, b] = w.edges ?? [0, 0];
      return t("res.warn.merged_walls", { a: a + 1, b: b + 1 });
    }
    case "plane_over_wall": {
      const [a, b] = w.edges ?? [0, 0];
      return t("res.warn.plane_over_wall", { a: a + 1, b: b + 1, v: num(w.value) });
    }
    case "merged_planes": {
      const [a, b] = w.edges ?? [0, 0];
      return t("res.warn.merged_planes", { a: a + 1, b: b + 1, v: num(w.value * 100) });
    }
    case "wing_joined":
      return t("res.warn.wing_joined", { walls: (w.edges ?? []).map((e) => e + 1).join(", ") });
    case "dormer_off_roof":
      return t("res.warn.dormer_off_roof", { n: (w.dormer ?? 0) + 1 });
    case "window_off_roof":
    case "chimney_off_roof":
    case "opening_off_roof":
      return t(`res.warn.${w.kind}`, { n: (w.item ?? 0) + 1 });
    case "area_mismatch":
      return t("res.warn.area_mismatch", { v: num(w.value * 100) });
    default:
      return t(`res.warn.${w.kind}`, { v: num(w.value) });
  }
}

/** One row of the Results area list: a part, or a face added by hand. */
export interface AreaRow {
  name: string;
  area: number;
  /** Index into `result.parts`, or -1 for a hand-drawn face. */
  part: number;
  manual: boolean;
}

/**
 * The Results area list: every part's area, then every hand-drawn face's
 * (the core's `manual_faces`, which the total already includes), so the
 * rows add up to the total.
 */
export function areaRows(result: CalcResult | null): AreaRow[] {
  if (!result) return [];
  const parts = result.parts.map((p, i) => ({ name: partLabel(p.name, i), area: p.quantities.roof_area, part: i, manual: false }));
  const faces = (result.manual_faces ?? []).map((f, i) => ({
    name: f.label || t("manual.faceN", { n: i + 1 }),
    area: f.area,
    part: -1,
    manual: true,
  }));
  return [...parts, ...faces];
}

/** Per part, the parts its roof is joined with, as the calculation joined them. */
export function joinedWith(result: CalcResult | null): number[][] {
  return (result?.parts ?? []).map((p) => p.geometry.joined_with ?? []);
}
