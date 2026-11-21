import { numberLocale, t } from "./i18n.svelte";

// A value that rounds to zero is zero: a roof an object took away completely
// has an area a hair below zero, which Intl prints as "-0,00" (audit HUNTC-004).
const fmt = (v: number) =>
  new Intl.NumberFormat(numberLocale(), { minimumFractionDigits: 2, maximumFractionDigits: 2 }).format(Math.abs(v) < 0.005 ? 0 : v);

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

/** What the library knows of a job's roof figures. */
export interface CardFigures {
  roof_area: number;
  parts?: number;
  figures_pending?: boolean;
  /** Buildings the core gave no roof for. */
  parts_failed?: number;
}

/**
 * The state of a library card's figures: still being calculated, not
 * calculated (the core gave no roof), partly calculated (some buildings
 * have no roof, the area leaves them out) or complete (ERRU-005).
 */
export function cardRoof(p: CardFigures): "pending" | "none" | "partial" | "ok" {
  if (p.figures_pending) return "pending";
  const failed = p.parts_failed ?? 0;
  if (failed <= 0) return "ok";
  return failed >= (p.parts ?? failed) ? "none" : "partial";
}

/** The area on a library card: "…" while the roof is still being calculated, "—" when the core gave none. */
export function cardArea(p: CardFigures): string {
  const state = cardRoof(p);
  return state === "pending" ? "…" : state === "none" || noBuilding(p) ? "—" : m2(p.roof_area);
}

/** A job with no building and no area (no hand-drawn face either): nothing to show as 0,00 m² (UX-111). */
function noBuilding(p: CardFigures): boolean {
  return p.parts === 0 && Math.abs(p.roof_area) < 0.005;
}

/** Why a card's area is not the whole roof, for its tooltip and the status mark; undefined when it is. */
export function cardAreaNote(p: CardFigures): string | undefined {
  const state = cardRoof(p);
  if (state === "pending") return t("lib.areaPending");
  if (state === "none") return t("res.notCalculated");
  if (noBuilding(p)) return t("res.empty");
  if (state === "partial") return t("lib.areaPartial", { n: p.parts_failed ?? 0, of: p.parts ?? 0 });
  return undefined;
}

/** Delay before listing the library again while a card waits for its figures, ms; null to stop asking. */
export function pendingRetryMs(pending: boolean, tries: number): number | null {
  if (!pending || tries >= 8) return null;
  return Math.min(1000 * 2 ** tries, 30000);
}

/** A file size in the given locale's number format: "512 B", "152 kB", "1,5 MB" (UI-010). */
export function fileSize(b: number, locale: string = numberLocale()): string {
  const f = (v: number, d: number) => new Intl.NumberFormat(locale, { maximumFractionDigits: d, minimumFractionDigits: d }).format(v);
  if (!Number.isFinite(b) || b < 0) return "—";
  if (b < 1024) return `${f(b, 0)} B`;
  if (b < 1048576) return `${f(b / 1024, 0)} kB`;
  return `${f(b / 1048576, 1)} MB`;
}
