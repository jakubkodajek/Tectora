// The offer's figures in the library: a job with a stored result lists the
// area the customer was offered, says the current one in the tooltip and
// marks the job when the roof changed since (review UX-008). Both figures
// and the "differs" flag come from the core's library summary.

import { t } from "./i18n.svelte";
import type { Project, ProjectSummary } from "./types";

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
