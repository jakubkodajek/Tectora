// Values a roofer typed that are allowed but unlikely: a lost decimal comma
// ("5" for 0,5 m of overhang) is taken without a word otherwise. The value
// stays; the field gets a small mark and the Results panel a line
// (review UX-009). Also a half-hip or pitch break at or above the roof's
// ridge, which the roof calculation leaves without effect (review UX-003).
// Input checks only: the ridge height comes from the core's result. The core
// computes the same (`tectora_core::notices`, which the MCP server reports as
// `input_notices`); `editparity.test.ts` checks both agree.

import { num } from "./format";
import { roundTo } from "./rounding";
import { t } from "./i18n.svelte";
import { partLabel } from "./partname";
import type { CalcResult, EdgeDef, Project } from "./types";
import { isGable, isMarker } from "./types";

/** Above these a value is marked as unusual, m [no source: a few times what TERRAN's roofs have]. */
export const UNUSUAL = {
  /** Overhangs are well under 1,5 m; 3 m catches "5" typed for 0,5. */
  overhang: 3,
  /** Eave and building heights of family houses and halls stay under 30 m. */
  height: 30,
  /** A wall over 200 m is a lost comma in a length. */
  length: 200,
} as const;

export type UnusualField = "overhang" | "height" | "length";

/** Whether a typed value of `field` is unusually large (it is still stored). */
export function isUnusual(field: UnusualField, v: number | null | undefined): boolean {
  return typeof v === "number" && Number.isFinite(v) && v > UNUSUAL[field];
}

export interface UnusualValue {
  part: number;
  edge: number;
  field: UnusualField;
  value: number;
}

/** Every wall value of the job that is unusually large, in wall order. */
export function unusualValues(project: Pick<Project, "parts">): UnusualValue[] {
  const out: UnusualValue[] = [];
  project.parts.forEach((p, pi) => {
    const n = p.edges.length;
    p.edges.forEach((e, ei) => {
      if (isMarker(e)) return;
      if (isUnusual("overhang", e.overhang)) out.push({ part: pi, edge: ei, field: "overhang", value: e.overhang });
      if (isUnusual("height", e.eave_height)) out.push({ part: pi, edge: ei, field: "height", value: e.eave_height });
      const b = p.edges[(ei + 1) % n].start;
      const len = Math.hypot(b.x - e.start.x, b.y - e.start.y);
      if (isUnusual("length", len)) out.push({ part: pi, edge: ei, field: "length", value: len });
    });
  });
  return out;
}

/** The highest point of a part's calculated roof (its ridge), m; null without a roof. */
export function ridgeOf(result: CalcResult | null | undefined, part: number): number | null {
  const p = result?.parts[part];
  if (!p || p.geometry.error || !(p.geometry.faces?.length ?? 0)) return null;
  const h = p.quantities.ridge_height;
  return Number.isFinite(h) && h > 0 ? h : null;
}

/** Tolerance for "at the ridge", m (1 cm). */
const AT_RIDGE = 0.01;

/** Whether a height lies at or above the ridge, where a half-hip or break changes nothing. */
export function aboveRidge(height: number, ridge: number | null): boolean {
  return ridge !== null && Number.isFinite(height) && height >= ridge - AT_RIDGE;
}

/**
 * The height a new half-hip starts at: halfway between the wall's eave and
 * the ridge, so ticking the box changes the roof; without a ridge the old
 * default (2,5 m above the eave).
 */
export function halfHipDefault(edge: Pick<EdgeDef, "eave_height">, ridge: number | null): number {
  if (ridge !== null && ridge > edge.eave_height + 0.1) return roundTo((edge.eave_height + ridge) / 2, 2);
  return roundTo(edge.eave_height + 2.5, 2);
}

/**
 * The height a new pitch break starts at: 1,5 m above the highest one so
 * far, but below the ridge when that would lie at or above it.
 */
export function breakDefault(top: number, ridge: number | null): number {
  const h = top + 1.5;
  if (ridge !== null && ridge > top + 0.1 && h >= ridge - AT_RIDGE) return roundTo((top + ridge) / 2, 2);
  return roundTo(h, 2);
}

/** Pitch of a new half-hip, degrees (core `notices::HALF_HIP_DEFAULT_PITCH`). */
export const HALF_HIP_PITCH = 50;

/** Pitch of a new pitch break: the pitch below it less 20°, within 5..85° (core `notices::break_default_pitch`). */
export const breakDefaultPitch = (below: number) => Math.max(5, Math.min(85, below - 20));

export interface RidgeIssue {
  part: number;
  edge: number;
  /** -1 for the half-hip, else the break's index. */
  brk: number;
  ridge: number;
}

/** Half-hips and pitch breaks at or above their roof's ridge, in wall order. */
export function aboveRidgeIssues(project: Pick<Project, "parts">, result: CalcResult | null | undefined): RidgeIssue[] {
  const out: RidgeIssue[] = [];
  project.parts.forEach((p, pi) => {
    const ridge = ridgeOf(result, pi);
    if (ridge === null) return;
    p.edges.forEach((e, ei) => {
      if (isMarker(e)) return;
      const gable = isGable(e);
      if (gable && e.half_hip && aboveRidge(e.half_hip.height, ridge)) out.push({ part: pi, edge: ei, brk: -1, ridge });
      if (!gable || e.half_hip) {
        (e.breaks ?? []).forEach((b, bi) => {
          if (aboveRidge(b.height, ridge)) out.push({ part: pi, edge: ei, brk: bi, ridge });
        });
      }
    });
  });
  return out;
}

/**
 * The Results lines for the values above, translated: one per unusual value
 * and per half-hip or break above the ridge, with the building's name when
 * the job has several.
 */
export function inputWarnings(project: Pick<Project, "parts"> | null | undefined, result: CalcResult | null | undefined): { part: string | null; text: string }[] {
  if (!project) return [];
  const many = project.parts.length > 1;
  const name = (pi: number) => (many ? partLabel(project.parts[pi]?.name ?? "", pi) : null);
  const out = unusualValues(project).map((u) => ({ part: name(u.part), text: t(`res.warn.unusual_${u.field}`, { n: u.edge + 1, v: num(u.value) }) }));
  for (const r of aboveRidgeIssues(project, result)) {
    const text =
      r.brk < 0
        ? t("res.warn.half_hip_above_ridge", { n: r.edge + 1, v: num(r.ridge) })
        : t("res.warn.break_above_ridge", { n: r.edge + 1, k: r.brk + 1, v: num(r.ridge) });
    out.push({ part: name(r.part), text });
  }
  return out;
}
