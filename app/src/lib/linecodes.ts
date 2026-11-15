// Names of the roof line codes (T eave, F ridge, G hip, K valley, O verge
// with OL / OR / OS, P shed top edge, c pitch break, S horizontal valley,
// W wall abutment with WV / WS / WH) in the UI language, one place for the
// Results legend, the Calculation tab, the Export dialog and the sheet's
// legend (review UX-013: WV, WS and WH were all "wall abutment").
import { t } from "./i18n.svelte";

const KEY: Record<string, string> = {
  T: "res.eave",
  F: "res.ridge",
  G: "res.hipShort",
  K: "res.valleyShort",
  O: "res.verge",
  OL: "bd.code.OL",
  OR: "bd.code.OR",
  OS: "bd.code.OS",
  P: "res.pult",
  c: "res.knick",
  S: "res.gorge",
  W: "res.abutment",
  WV: "line.WV",
  WS: "line.WS",
  WH: "line.WH",
};

/** Every code with a name of its own, in the order of the Results table. */
export const NAMED_CODES = Object.keys(KEY);

/** The name of line code `code`; codes without one of their own are "other line". */
export function lineName(code: string): string {
  return t(KEY[code] ?? "line.other");
}

/** Code → name of every named code, for the legend of the plan sheet. */
export function lineNames(): Record<string, string> {
  return Object.fromEntries(NAMED_CODES.map((c) => [c, lineName(c)]));
}

/** The sub-codes of a Results row (O: OL, OR, OS; W: WV, WS, WH), in this order. */
export const SUB_CODES: Record<string, string[]> = { O: ["OL", "OR", "OS"], W: ["WV", "WS", "WH"] };

/**
 * The legend under the Results lines: "T Okapy, F Hřebeny, …" for the rows
 * shown, with the sub-codes the roof has (`codes`: the core's lengths by
 * code), so every code on screen is named and none that is not.
 */
export function legend(rows: string[], codes: Record<string, number>): string {
  const has = (c: string) => Math.abs(codes[c] ?? 0) >= 0.005;
  const items = rows.map((r) => {
    const subs = (SUB_CODES[r] ?? []).filter(has);
    // Names as they are: German nouns keep their capital.
    const name = lineName(r);
    return subs.length ? `${r} ${name} (${subs.map((s) => `${s} ${lineName(s)}`).join(", ")})` : `${r} ${name}`;
  });
  return t("res.legend", { list: items.join(", ") });
}
