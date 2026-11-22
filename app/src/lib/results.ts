// What the Results panel says about a calculation besides its figures: the
// warnings of each part and the parts each roof is joined with. Both come
// from the core (`check.rs`, the Modellverschnitt's own join); nothing here
// guesses from the plan.

import { num } from "./format";
import { t } from "./i18n.svelte";
import { partLabel } from "./partname";
import type { CalcResult, Project, RoofWarning } from "./types";
import { hasBuilding } from "./offer";

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
    case "far_from_origin":
      return t("res.warn.far_from_origin", { v: num(w.value / 1000) });
    case "objects_overlap":
      return t("res.warn.objects_overlap", { n: Math.round(w.value) });
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

/**
 * Whether the core refused the roof (a part's calculation ended in an
 * error): its figures are zeros, not a result, so they are shown as "—"
 * (review ERR-004). True when any part failed; see `roofState` for the
 * other parts.
 */
export function notCalculated(result: CalcResult | null): boolean {
  return !!result?.parts.some((p) => p.geometry.error);
}

/**
 * The parts without a roof, by index into `result.parts`: the core gave
 * none, or the part was left out of the calculation for its input (a
 * problem of the part; it then carries no error, only zeros; ERR-001).
 */
export function failedParts(result: CalcResult | null): number[] {
  const leftOut = new Set((result?.problems ?? []).filter((p) => p.item === "part").map((p) => p.index));
  return (result?.parts ?? []).flatMap((p, i) => (p.geometry.error || leftOut.has(i) ? [i] : []));
}

/**
 * "none": no part has a roof, the figures are zeros; "partial": some parts
 * have none and the totals are the other parts' (the core calculates them
 * without the failed ones, ERRU-003); "ok": every part has its roof.
 */
export function roofState(result: CalcResult | null): "none" | "partial" | "ok" {
  const failed = failedParts(result).length;
  if (!result || failed === 0) return "ok";
  return failed >= result.parts.length ? "none" : "partial";
}

/**
 * Whether there is no roof yet: no building and no hand-drawn face (an empty
 * job, or a footprint still being drawn). Its zeros are no figures (UX-011).
 */
export function emptyRoof(result: CalcResult | null): boolean {
  return !result || (result.parts.length === 0 && !(result.manual_faces?.length));
}

/**
 * Why Results has no figures to show: "empty" for a job without a building
 * (UX-011), "calculating" while the first result for a job with buildings
 * is on its way, "failed" when the calculation of the whole job failed
 * (too complex, an internal error; UX-101), else null. Without the job
 * (`project` null) only the result decides, as `emptyRoof`.
 */
export function missingResult(
  project: Pick<Project, "parts" | "manual_faces"> | null,
  result: CalcResult | null,
  failure: unknown,
): "empty" | "calculating" | "failed" | null {
  if (project ? !hasBuilding(project) : emptyRoof(result)) return "empty";
  if (result) return null;
  return failure ? "failed" : "calculating";
}

/** A roof of hand-drawn faces only, no building: it has no ridge and no lines of its own (UX-302). */
export function isFacesOnly(project: Pick<Project, "parts" | "manual_faces"> | null | undefined): boolean {
  return !!project && project.parts.length === 0 && (project.manual_faces?.length ?? 0) > 0;
}

/**
 * The line rows the results list: all of them for a building (a zero says
 * "no hips"), only the ones with a length for a roof of hand-drawn faces
 * only, where rows of 0,00 m would read as calculated lines (UX-302).
 */
export function lineRowsShown<T extends { value: number }>(rows: T[], facesOnly: boolean): T[] {
  return facesOnly ? rows.filter((r) => r.value >= 0.005) : rows;
}
