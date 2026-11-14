// The bounds of the roof values a person types, one set for every place
// that takes them (New job form, draw bar presets, wall bar), so a typo such
// as 50 m of overhang is refused the same way everywhere (review UX-001).
// Plausibility bounds of the UI, narrower than what the core takes
// (`model::MAX_ABS_VALUE`); the core keeps its own checks.

import { formatNumber } from "./numeric.svelte";
import { numberLocale, t } from "./i18n.svelte";

export interface Bounds {
  min: number;
  max: number;
}

/** Overhang, m. */
export const OVERHANG: Bounds = { min: 0, max: 20 };
/** Eave height of the presets and the New job form, m (below the ground for a building on a slope). */
export const EAVE_HEIGHT: Bounds = { min: -100, max: 1000 };
/** Building height, eave height, pitch-break and half-hip heights of a wall, m. */
export const WALL_HEIGHT: Bounds = { min: 0, max: 1000 };

/**
 * True when `v` lies in `b`; otherwise the field's refusal ("Allowed range
 * 0 to 20"), the same sentence `numeric` shows for its own bounds.
 */
export function within(v: number, b: Bounds): true | string {
  if (Number.isFinite(v) && v >= b.min && v <= b.max) return true;
  const f = (x: number) => formatNumber(x, numberLocale());
  return t("num.range", { min: f(b.min), max: f(b.max) });
}
