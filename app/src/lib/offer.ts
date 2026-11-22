// The offer's figures in the library: a job with a stored result lists the
// area the customer was offered, says the current one in the tooltip and
// marks the job when the roof changed since (review UX-008). Both figures
// and the "differs" flag come from the core's library summary.

import { t } from "./i18n.svelte";
import type { FigureDifference, Project, ProjectSummary, ResultSnapshot } from "./types";

type OfferFigures = Pick<ProjectSummary, "has_result" | "stored_area" | "result_differs">;

/** The stored result's area, m², or null when the job has none. */
export function storedArea(p: OfferFigures): number | null {
  return p.has_result && typeof p.stored_area === "number" && Number.isFinite(p.stored_area) ? p.stored_area : null;
}

/** Whether the current roof differs from the stored result's figures. */
export function offerDiffers(p: OfferFigures): boolean {
  return storedArea(p) !== null && !!p.result_differs;
}

/** The area the library lists and sorts by: the offer's for a job with a stored result, else the current one. */
export function listedArea(p: OfferFigures & Pick<ProjectSummary, "roof_area">): number {
  return storedArea(p) ?? p.roof_area;
}

/** The tooltip of an offer's listed area: that it is the offer's, and the current calculation (`current`, already formatted). */
export function storedAreaHint(current: string): string {
  return t("lib.storedAreaHint", { v: current });
}

/**
 * What the stored result is calculated from: the job without its texts,
 * status and stored result. Two projects with the same value give the
 * same figures, so a result taken from one can be stored on the other
 * (an AI save of the notes while the status changes, ERR-102).
 */
export function roofInput(p: Project): string {
  const { name: _n, notes: _o, job: _j, result_snapshot: _r, ...rest } = p;
  return JSON.stringify(rest);
}

/** Whether the job has anything to calculate: a building or a hand-drawn face (UX-102, UX-111). */
export function hasBuilding(p: Pick<Project, "parts" | "manual_faces">): boolean {
  return p.parts.length > 0 || (p.manual_faces?.length ?? 0) > 0;
}

/** Smallest change of a figure that counts, m or m² (core `snapshot::TOLERANCE`). */
const TOLERANCE = 0.005;
/** The material every face has unless the user picks another (core `model::DEFAULT_MATERIAL`). */
const DEFAULT_MATERIAL = 1;

/**
 * What differs between two results the core took, by the rule of the
 * core's `snapshot::compare` and in its order (area, ridge height, every
 * line code, and the areas by material once the roof has more than the
 * default one): a job offered again keeps its stored result, and the user
 * is told which figure the roof now gives differently (UX-206, ERR-304).
 * Comparison only, no roof math; held to the core by the shared cases in
 * `tests/fixtures/parity/snapshot_compare_cases.json` (TEST-302).
 */
export function snapshotDifferences(stored: ResultSnapshot, current: ResultSnapshot): FigureDifference[] {
  const out: FigureDifference[] = [];
  const check = (key: string, a: number, b: number) => {
    if (Math.abs(a - b) >= TOLERANCE) out.push({ key, stored: a, current: b });
  };
  check("roof_area", stored.roof_area, current.roof_area);
  check("ridge_height", stored.ridge_height, current.ridge_height);
  // The core's order: codes as its BTreeSet<String> sorts them (by code unit), materials by number.
  const codes = [...new Set([...Object.keys(stored.lines_by_code), ...Object.keys(current.lines_by_code)])].sort((a, b) => (a < b ? -1 : a > b ? 1 : 0));
  for (const c of codes) check(`line:${c}`, stored.lines_by_code[c] ?? 0, current.lines_by_code[c] ?? 0);
  const mats = [...new Set([...stored.area_by_material, ...current.area_by_material].map((m) => m.material))].sort((a, b) => a - b);
  // One default material is the roof area again; only a split says more.
  if (mats.every((m) => m === DEFAULT_MATERIAL)) return out;
  const area = (s: ResultSnapshot, m: number) => s.area_by_material.find((x) => x.material === m)?.area ?? 0;
  for (const m of mats) check(`material:${m}`, area(stored, m), area(current, m));
  return out;
}

/** Whether the figures of two results the core took differ (`snapshotDifferences`). */
export function snapshotDiffers(stored: ResultSnapshot, current: ResultSnapshot): boolean {
  return snapshotDifferences(stored, current).length > 0;
}
