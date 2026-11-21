// The roof-line details the editor shows when the pointer is on a ridge,
// eave, hip, … in the plan or in 3D. The figures come from the core
// (`lineinfo::LineDetail`, in the calculation result); this file only
// finds a line's details and names it in the UI language.

import { t } from "./i18n.svelte";
import { materialName, materialToken } from "./materials";
import { wallNumber } from "./planlabels";
import type { CalcResult, LineDetail, LineRef, Project } from "./types";

/** Codes named by their own key (the verge sides and the abutment kinds). */
const OWN: Record<string, string> = {
  OL: "bd.code.OL",
  OR: "bd.code.OR",
  OS: "bd.code.OS",
  WV: "line.WV",
  WS: "line.WS",
  WH: "line.WH",
};
/** Codes with a singular name ("Hřeben", not the Results' "Hřebeny"). */
const ONE = new Set(["T", "F", "G", "K", "O", "P", "c", "S", "W"]);

/** The name of one line of code `code` in the UI language. */
export function lineTitle(code: string): string {
  if (OWN[code]) return t(OWN[code]);
  return ONE.has(code) ? t(`lineinfo.${code}`) : t("line.other");
}

export const sameLine = (a: LineRef | null | undefined, b: LineRef | null | undefined) =>
  !!a && !!b && a.part === b.part && a.index === b.index;

/** The details of line `ref` in `result`, or null (no such line, or an older result without details). */
export function detailOf(result: CalcResult | null, ref: LineRef | null): LineDetail | null {
  if (!result || !ref) return null;
  return result.line_details?.find((d) => sameLine(d, ref)) ?? null;
}

/** A line counts as level below this slope, degrees [no source: display threshold, 1 mm per 2 m]. */
export const LEVEL_DEG = 0.03;

/** Roof line kind → its colour token (plan, 3D and the details' swatch). */
export const LINE_TOKENS: Record<string, string> = {
  eave: "--line-eave",
  verge: "--line-verge",
  ridge: "--line-ridge",
  hip: "--line-hip",
  valley: "--line-valley",
  pult: "--line-pult",
  knick: "--line-knick",
  gorge: "--line-gorge",
  abutment: "--line-abutment",
  abutment_front: "--line-abutment",
  abutment_back: "--line-abutment",
  abutment_side: "--line-abutment",
};

/** A line counts as cut by a chimney, opening or dormer from this difference, m (the results round to cm). */
const UNCOUNTED_M = 0.005;

/** What the line card shows, as plain values (LineInfo.svelte renders them). */
export interface LineFigures {
  /** The length the results count, m. */
  length: number;
  /** The plan length when it differs from the true one (a sloping line), m. */
  planLength: number | null;
  /** The drawn length when part of the line is not counted, m. */
  drawn: number | null;
  level: boolean;
  zLow: number;
  zHigh: number;
  slopeDeg: number;
  faces: { token: string; label: string; pitchDeg: number; material: string }[];
}

/** The figures of `d` as the card shows them. */
export function lineFigures(d: LineDetail, project: Project): LineFigures {
  const counted = d.counted_length ?? d.length;
  const parts = project.parts.length;
  return {
    length: counted,
    planLength: Math.abs(d.length - d.plan_length) >= UNCOUNTED_M ? d.plan_length : null,
    drawn: d.length - counted >= UNCOUNTED_M ? d.length : null,
    level: d.slope_deg < LEVEL_DEG,
    zLow: d.z_low,
    zHigh: d.z_high,
    slopeDeg: d.slope_deg,
    faces: d.faces.map((f) => ({
      token: materialToken(f.material),
      label: f.wall === undefined ? t("lineinfo.dormer") : t("lineinfo.wall", { n: wallNumber(f.part, f.wall, parts) }),
      pitchDeg: f.pitch_deg,
      material: materialName(project, f.material),
    })),
  };
}
