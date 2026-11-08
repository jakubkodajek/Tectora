// What the Results panel says about a calculation besides its figures: the
// warnings of each part and the parts each roof is joined with. Both come
// from the core (`check.rs`, the Modellverschnitt's own join); nothing here
// guesses from the plan.

import { num } from "./format";
import { t } from "./i18n.svelte";
import type { CalcResult, RoofWarning } from "./types";

/** The translated sentence of one plausibility warning. */
export function warningText(w: RoofWarning): string {
  switch (w.kind) {
    case "merged_walls": {
      const [a, b] = w.edges ?? [0, 0];
      return t("res.warn.merged_walls", { a: a + 1, b: b + 1 });
    }
    case "dormer_off_roof":
      return t("res.warn.dormer_off_roof", { n: (w.dormer ?? 0) + 1 });
    case "area_mismatch":
      return t("res.warn.area_mismatch", { v: num(w.value * 100) });
    default:
      return t(`res.warn.${w.kind}`, { v: num(w.value) });
  }
}

/** Per part, the parts its roof is joined with, as the calculation joined them. */
export function joinedWith(result: CalcResult | null): number[][] {
  return (result?.parts ?? []).map((p) => p.geometry.joined_with ?? []);
}
