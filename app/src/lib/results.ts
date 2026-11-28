// What the Results panel says about a calculation besides its figures: the
// warnings of each part and the parts each roof is joined with. Both come
// from the core (`check.rs`, the Modellverschnitt's own join); nothing here
// guesses from the plan.

import { num } from "./format";
import { t } from "./i18n.svelte";
import { partLabel } from "./partname";
import type { CalcResult, Project, RoofWarning, RoofWindow } from "./types";
import { hasBuilding } from "./offer";

/** The translated sentence of one plausibility warning. */
export function warningText(w: RoofWarning, result: CalcResult | null = null): string {
  switch (w.kind) {
    case "tower_base_below_roof":
    case "tower_eave_below_roof":
      // The core's warnings for a tower roof sunk into the roof (UX-002) or its eave under it (r3 MATH-005), by the tower's name.
      return t(`res.warn.${w.kind}`, { name: (w.towers ?? []).map((i) => result?.towers?.[i]?.label || t("tower.n", { n: i + 1 })).join(", "), v: num(w.value) });
    case "merged_walls": {
      const [a, b] = w.edges ?? [0, 0];
      return t("res.warn.merged_walls", { a: a + 1, b: b + 1 });
    }
    case "merged_materials": {
      const [a, b] = w.edges ?? [0, 0];
      return t("res.warn.merged_materials", { a: a + 1, b: b + 1 });
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
    case "manual_face_crosses":
      return t("res.warn.manual_face_crosses", { n: (w.face ?? 0) + 1 });
    case "manual_faces_overlap":
      return t("res.warn.manual_faces_overlap", { a: (w.face ?? 0) + 1, b: (w.other_face ?? 0) + 1, v: num(w.value) });
    case "manual_face_over_roof":
      return t("res.warn.manual_face_over_roof", { n: (w.face ?? 0) + 1, v: num(w.value) });
    case "area_mismatch":
      return t("res.warn.area_mismatch", { v: num(w.value * 100) });
    case "far_from_origin":
      return t("res.warn.far_from_origin", { v: num(w.value / 1000) });
    case "objects_overlap":
      return t("res.warn.objects_overlap", { n: Math.round(w.value) });
    case "no_roof_area":
      // A tower's shaft took the roof: name it, not a chimney (ERR-005).
      if (w.towers?.length) return t("res.warn.no_roof_area_tower", { names: w.towers.map((i) => t("tower.n", { n: i + 1 })).join(", ") });
      return t("res.warn.no_roof_area", { v: num(w.value) });
    default:
      return t(`res.warn.${w.kind}`, { v: num(w.value) });
  }
}

/**
 * The core's notices: a hint about the input or what Tectora did beyond the
 * standard calculation, not a sign that the roof is wrong (core
 * `WarningKind::is_notice`).
 */
const NOTICES = new Set<RoofWarning["kind"]>(["plane_over_wall", "merged_planes", "wing_joined", "far_from_origin", "manual_face_over_roof"]);

/**
 * Whether the calculation holds a warning that is not a notice: the Results tab is marked (UX-1401). Also the
 * warnings that belong to no building (a job of hand-drawn faces only, ERR-1901; audit 2026-11-24 UI-2002).
 */
export function hasRoofWarning(result: CalcResult | null): boolean {
  const warned = (ws: RoofWarning[] | undefined) => (ws ?? []).some((w) => !NOTICES.has(w.kind));
  return (result?.parts ?? []).some((p) => warned(p.geometry.warnings)) || warned(result?.warnings);
}

/** How many towers the core cut out of a roof (their shafts are deducted with the chimneys, MATH-005). */
export function towersOnRoof(result: CalcResult | null): number {
  return (result?.towers ?? []).filter((tw) => tw.result && tw.shaft?.on_roof).length;
}

/**
 * The core's warning that window, chimney or roof opening `index` lies on
 * no roof face (`check::off_roof`), or null. Nothing is guessed from the
 * plan here: the test is the core's (UX-1401).
 */
export function offRoofWarning(result: CalcResult | null, kind: "window" | "chimney" | "opening", index: number): RoofWarning | null {
  const want = `${kind}_off_roof`;
  for (const p of result?.parts ?? []) {
    const w = (p.geometry.warnings ?? []).find((w) => w.kind === want && w.item === index);
    if (w) return w;
  }
  return null;
}

/** Windows, chimneys, openings and dormers of a job, split by whether the calculation placed them on the roof. */
export interface ObjectCounts {
  /** The windows on the roof (their sizes are listed). */
  windows: RoofWindow[];
  chimneys: number;
  holes: number;
  dormers: number;
  /** Per kind, how many the core reported on no roof face (`*_off_roof`). */
  off: { windows: number; chimneys: number; holes: number; dormers: number };
}

/**
 * What the Results panel lists as on the roof: every object of the job but
 * the ones the core's warnings say lie on no roof face, which are counted
 * apart (audit 2026-11-23 UX-1503). From the core's warnings only; nothing
 * is judged from the plan here.
 */
export function objectCounts(project: Project, result: CalcResult | null): ObjectCounts {
  const off = { window: new Set<number>(), chimney: new Set<number>(), opening: new Set<number>(), dormer: new Set<number>() };
  for (const p of result?.parts ?? []) {
    for (const w of p.geometry.warnings ?? []) {
      if (w.kind === "dormer_off_roof" && w.dormer !== undefined) off.dormer.add(w.dormer);
      else if ((w.kind === "window_off_roof" || w.kind === "chimney_off_roof" || w.kind === "opening_off_roof") && w.item !== undefined) {
        off[w.kind.replace("_off_roof", "") as "window" | "chimney" | "opening"].add(w.item);
      }
    }
  }
  // Only indices the job has (a warning of an older result is not counted).
  const within = (s: Set<number>, n: number) => [...s].filter((i) => i >= 0 && i < n).length;
  const windows = project.windows ?? [];
  const n = { chimneys: project.chimneys?.length ?? 0, holes: project.roof_openings?.length ?? 0, dormers: project.dormers?.length ?? 0 };
  const o = {
    windows: within(off.window, windows.length),
    chimneys: within(off.chimney, n.chimneys),
    holes: within(off.opening, n.holes),
    dormers: within(off.dormer, n.dormers),
  };
  return {
    windows: windows.filter((_, i) => !off.window.has(i)),
    chimneys: n.chimneys - o.chimneys,
    holes: n.holes - o.holes,
    dormers: n.dormers - o.dormers,
    off: o,
  };
}

/** One row of the Results area list: a part, or a face added by hand. */
export interface AreaRow {
  name: string;
  area: number;
  /** Index into `result.parts`, or -1 for a hand-drawn face or a tower. */
  part: number;
  manual: boolean;
  /** A tower's roof (the core's `towers`, already in the total). */
  tower?: boolean;
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
  const towers = (result.towers ?? []).flatMap((tw, i) =>
    tw.result ? [{ name: tw.label || t("tower.n", { n: i + 1 }), area: tw.roof_area ?? 0, part: -1, manual: false, tower: true }] : [],
  );
  return [...parts, ...faces, ...towers];
}

/** One tier of a tower in Results. */
export interface TowerTierRow {
  n: number;
  /** A vertical tier (a lantern) counts as wall, not as roof. */
  wall: boolean;
  pitch_deg: number;
  area: number;
  material: number;
  /** A rectangle that is not square: the pitch and area of its sides along the depth (part of `area`). */
  depth?: { pitch_deg: number; area: number };
}

/** One tower in Results: what its roof, its lantern and its shaft take (core `report::tower_report`). */
export interface TowerRow {
  name: string;
  /** False for a tower with problems: no figures. */
  calculated: boolean;
  roof_area: number;
  wall_area: number;
  shaft_area: number;
  top_height: number;
  cap: number | null;
  on_ground: boolean;
  /** The building under the tower has no calculated roof: no cladding figure. */
  roof_failed: boolean;
  base_below_roof: boolean;
  tiers: TowerTierRow[];
}

/**
 * The Results tower list: per tower its roof area (already in the total and
 * in the area list), the vertical walls of its lanterns, the cladding of
 * its shaft above the roof and each tier.
 */
export function towerRows(result: CalcResult | null): TowerRow[] {
  return (result?.towers ?? []).map((tw, i) => {
    const r = tw.result;
    return {
      name: tw.label || t("tower.n", { n: i + 1 }),
      calculated: !!r,
      roof_area: tw.roof_area ?? 0,
      wall_area: tw.wall_area ?? 0,
      shaft_area: tw.shaft?.wall_area ?? 0,
      top_height: r?.top_height ?? 0,
      cap: r?.cap ? r.cap.area : null,
      // A building without a roof under it is not "beside the roofs" (ERR-005).
      on_ground: !!tw.shaft && !tw.shaft.on_roof && !tw.shaft.roof_failed,
      roof_failed: !!tw.shaft?.roof_failed,
      base_below_roof: !!tw.shaft?.base_below_roof,
      tiers: (r?.tiers ?? []).map((x) => ({
        n: x.index + 1,
        wall: x.kind === "wall",
        pitch_deg: x.pitch_deg,
        area: x.area,
        material: x.material,
        ...(x.depth_sides ? { depth: { pitch_deg: x.depth_sides.pitch_deg, area: x.depth_sides.area } } : {}),
      })),
    };
  });
}

/**
 * Whether Results lists the areas: with more than one row, and also for the
 * one face of a job without a building, whose area is otherwise
 * unexplained (audit 2026-11-24 UX-1904).
 */
export function areaListShown(rows: AreaRow[], facesOnly: boolean): boolean {
  return rows.length > 1 || (facesOnly && rows.length > 0);
}

/**
 * The calculation's plausibility warnings: each part's, named after the
 * part when there are several, and those that belong to no building (a
 * crossing hand-drawn face in a job without one, ERR-1901), unnamed.
 */
export function resultWarnings(result: CalcResult | null): { part: string | null; text: string }[] {
  if (!result) return [];
  const parts = result.parts.flatMap((p, i) =>
    (p.geometry.warnings ?? []).map((w) => ({ part: result.parts.length > 1 ? partLabel(p.name, i) : null, text: warningText(w, result) })),
  );
  return parts.concat((result.warnings ?? []).map((w) => ({ part: null, text: warningText(w, result) })));
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
  return !result || (result.parts.length === 0 && !(result.manual_faces?.length) && !(result.towers?.length));
}

/**
 * Why Results has no figures to show: "empty" for a job without a building
 * (UX-011), "calculating" while the first result for a job with buildings
 * is on its way, "failed" when the calculation of the whole job failed
 * (too complex, an internal error; UX-101), else null. Without the job
 * (`project` null) only the result decides, as `emptyRoof`.
 */
export function missingResult(
  project: (Pick<Project, "parts" | "manual_faces"> & Partial<Pick<Project, "towers">>) | null,
  result: CalcResult | null,
  failure: unknown,
): "empty" | "calculating" | "failed" | null {
  if (project ? !hasBuilding(project) : emptyRoof(result)) return "empty";
  if (result) return null;
  return failure ? "failed" : "calculating";
}

/** A roof of hand-drawn faces or towers only, no building: it has no ridge and no lines of its own (UX-302). */
export function isFacesOnly(project: (Pick<Project, "parts" | "manual_faces"> & Partial<Pick<Project, "towers">>) | null | undefined): boolean {
  return !!project && project.parts.length === 0 && ((project.manual_faces?.length ?? 0) > 0 || (project.towers?.length ?? 0) > 0);
}

/**
 * The line rows the results list: all of them for a building (a zero says
 * "no hips"), only the ones with a length for a roof of hand-drawn faces
 * only, where rows of 0,00 m would read as calculated lines (UX-302).
 */
export function lineRowsShown<T extends { value: number }>(rows: T[], facesOnly: boolean): T[] {
  return facesOnly ? rows.filter((r) => r.value >= 0.005) : rows;
}
