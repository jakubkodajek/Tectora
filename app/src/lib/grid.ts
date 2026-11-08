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

/**
 * Spacing of the grid lines at zoom `scale` (px per m) for the base grid
 * `base` (m): the base, doubled when the lines come closer than 16 px, and
 * then powers of ten of it until they are at least 8 px apart, so a very
 * large part does not turn the plan into a grey field (UI-011).
 */
export function gridStep(scale: number, base: number): number {
  if (!(scale > 0) || !Number.isFinite(scale)) return base * 1e6;
  if (scale * base >= 16) return base;
  if (scale * base >= 8) return base * 2;
  let s = base * 10;
  while (scale * s < 8 && s < base * 1e9) s *= 10;
  return s;
}

/** A zoom (px per m) that fits `size` m into `px` pixels, or null when the view has no room. */
export function fitScale(widthPx: number, heightPx: number, w: number, h: number): number | null {
  if (!(widthPx >= 10) || !(heightPx >= 10)) return null;
  const s = Math.min((widthPx * 0.75) / w, (heightPx * 0.75) / h);
  return Number.isFinite(s) && s > 0 ? s : null;
}
