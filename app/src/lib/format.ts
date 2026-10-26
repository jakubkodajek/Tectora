import { numberLocale, t } from "./i18n.svelte";

const fmt = (v: number) =>
  new Intl.NumberFormat(numberLocale(), { minimumFractionDigits: 2, maximumFractionDigits: 2 }).format(v);

export const m = (v: number) => `${fmt(v)} m`;
export const m2 = (v: number) => `${fmt(v)} m²`;
export const num = fmt;

export function relativeTime(ms: number): string {
  const min = Math.round((Date.now() - ms) / 60000);
  if (min < 1) return t("time.now");
  if (min < 60) return t("time.min", { n: min });
  const h = Math.round(min / 60);
  if (h < 24) return t("time.h", { n: h });
  return new Date(ms).toLocaleDateString(numberLocale(), { day: "numeric", month: "short", year: "numeric" });
}
