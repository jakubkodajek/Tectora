/**
 * Grid line positions for the plan view, world coordinates. Pure, so the
 * bounds checks are unit-tested: a zero-height view once made the zoom 0,
 * the bounds infinite and the loop endless (UI-001).
 */

/** More lines than any screen shows at the coarsest grid. */
export const MAX_GRID_LINES = 2000;

/**
 * Multiples of `step` from `from` (rounded down) to `to`; empty when the
 * range is not finite, the step is not positive or there would be more than
 * `MAX_GRID_LINES`. O(lines).
 */
export function gridValues(from: number, to: number, step: number): number[] {
  if (!Number.isFinite(from) || !Number.isFinite(to) || !(step > 0) || to < from) return [];
  const start = Math.floor(from / step) * step;
  const n = Math.floor((to - start) / step) + 1;
  if (!(n <= MAX_GRID_LINES)) return [];
  return Array.from({ length: n }, (_, i) => start + i * step);
}

/** A zoom (px per m) that fits `size` m into `px` pixels, or null when the view has no room. */
export function fitScale(widthPx: number, heightPx: number, w: number, h: number): number | null {
  if (!(widthPx >= 10) || !(heightPx >= 10)) return null;
  const s = Math.min((widthPx * 0.75) / w, (heightPx * 0.75) / h);
  return Number.isFinite(s) && s > 0 ? s : null;
}
