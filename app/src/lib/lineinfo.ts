// The roof-line details the editor shows when the pointer is on a ridge,
// eave, hip, … in the plan or in 3D. The figures come from the core
// (`lineinfo::LineDetail`, in the calculation result); this file only
// finds a line's details and names it in the UI language.

import { t } from "./i18n.svelte";
import type { CalcResult, LineDetail, LineRef } from "./types";

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
