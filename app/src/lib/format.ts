import { numberLocale, t } from "./i18n.svelte";

const fmt = (v: number) =>
  new Intl.NumberFormat(numberLocale(), { minimumFractionDigits: 2, maximumFractionDigits: 2 }).format(v);

export const m = (v: number) => `${fmt(v)} m`;
export const m2 = (v: number) => `${fmt(v)} m²`;
export const num = fmt;

/**
 * An intermediate value of a calculation shown for checking: at most `max`
 * decimals with trailing zeros cut (but at least `min`), so the core's
 * single-precision noise (58.499997 for 13 × 4.5) reads as 58,50 (UX-014).
 */
export function plain(v: number, locale: string, max = 4, min = 2): string {
  return new Intl.NumberFormat(locale, { minimumFractionDigits: Math.min(min, max), maximumFractionDigits: max, useGrouping: false }).format(
    Object.is(v, -0) ? 0 : v,
  );
}

export function relativeTime(ms: number): string {
  const min = Math.round((Date.now() - ms) / 60000);
  if (min < 1) return t("time.now");
  if (min < 60) return t("time.min", { n: min });
  const h = Math.round(min / 60);
  if (h < 24) return t("time.h", { n: h });
  return new Date(ms).toLocaleDateString(numberLocale(), { day: "numeric", month: "short", year: "numeric" });
}

/** The area on a library card: "…" while the roof is still being calculated in the background. */
export const cardArea = (p: { roof_area: number; figures_pending?: boolean }) => (p.figures_pending ? "…" : m2(p.roof_area));

/** Delay before listing the library again while a card waits for its figures, ms; null to stop asking. */
export function pendingRetryMs(pending: boolean, tries: number): number | null {
  if (!pending || tries >= 8) return null;
  return Math.min(1000 * 2 ** tries, 30000);
}
