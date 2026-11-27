// A settings change an AI client made (MCP update_settings, core
// `Settings::ai_changes`, audit UX-003): the values before and after in the
// user's language, for the review card and the AI tab.

import { LOCALES, numberLocale, t } from "./i18n.svelte";
import type { AiSettingsChange, AppSettings } from "./types";

export interface ChangeRow {
  key: string;
  label: string;
  before: string;
  after: string;
  /** The value now, when it is no longer the recorded `after` (changed again since, UX-104). */
  now?: string;
}

/** The label of a recorded key (`locale`, `roof_defaults.pitch_deg`, …). */
function label(key: string): string {
  switch (key) {
    case "locale":
      return t("lang");
    case "theme":
      return t("settings.theme");
    case "follow_ai":
      return t("settings.followAi");
    case "roof_defaults.pitch_deg":
      return t("edge.pitch");
    case "roof_defaults.eave_height":
      return t("edge.eaveHeight");
    case "roof_defaults.overhang":
      return t("edge.overhang");
    default:
      return key;
  }
}

function number(v: number, decimals: number): string {
  return new Intl.NumberFormat(numberLocale(), { minimumFractionDigits: decimals, maximumFractionDigits: Math.max(decimals, 2) }).format(v);
}

/** A recorded value as the Settings screen shows it. */
export function changeValue(key: string, v: unknown): string {
  if (key === "locale") {
    if (v === null || v === undefined || v === "") return t("settings.theme.system");
    return LOCALES.find((l) => l.id === v)?.label ?? String(v);
  }
  if (key === "theme" && typeof v === "string" && ["system", "light", "dark"].includes(v)) return t(`settings.theme.${v}`);
  if (key === "follow_ai" && typeof v === "boolean") return t(v ? "aiChange.on" : "aiChange.off");
  if (typeof v === "number" && Number.isFinite(v)) {
    if (key === "roof_defaults.pitch_deg") return `${number(v, 0)}°`;
    if (key.startsWith("roof_defaults.")) return `${number(v, 2)} m`;
  }
  return v === null || v === undefined ? "–" : String(v);
}

/** The value of a recorded key in `s` (`roof_defaults.pitch_deg` → `s.roof_defaults.pitch_deg`). */
function currentValue(s: AppSettings, key: string): unknown {
  const sub = key.startsWith("roof_defaults.") ? key.slice("roof_defaults.".length) : null;
  const v = sub !== null ? (s.roof_defaults as unknown as Record<string, unknown> | undefined)?.[sub] : (s as unknown as Record<string, unknown>)[key];
  return v === undefined ? null : v;
}

/** One row per changed setting: label, before, after, and the value now when `current` has another. */
export function changeRows(c: AiSettingsChange, current?: AppSettings | null): ChangeRow[] {
  return Object.keys(c.after).map((key) => {
    const row: ChangeRow = {
      key,
      label: label(key),
      before: changeValue(key, c.before[key]),
      after: changeValue(key, c.after[key]),
    };
    if (current) {
      const now = changeValue(key, currentValue(current, key));
      if (now !== row.after) row.now = now;
    }
    return row;
  });
}

const isRoofKey = (key: string) => key.startsWith("roof_defaults.");

/** A row's label on the card: roof defaults under Settings' "New roofs", not like the open roof's own fields (UX-105). */
export function rowLabel(r: ChangeRow): string {
  return isRoofKey(r.key) ? `${t("settings.newRoofs")}: ${r.label}` : r.label;
}

/** The card's note: that only new buildings take changed roof defaults; nothing about roofs otherwise (UX-105). */
export function changeNote(c: AiSettingsChange): string {
  return t(Object.keys(c.after).some(isRoofKey) ? "aiChange.noteRoofs" : "aiChange.note");
}

/** The state in the history: a Revert that kept every value put nothing back (UX-104). */
export function changeState(c: AiSettingsChange): string {
  const keys = Object.keys(c.after);
  if (c.state === "reverted" && keys.length > 0 && keys.every((k) => c.kept?.includes(k))) return t("aiChange.state.nothing");
  return t(`aiChange.state.${c.state}`);
}

/** What a Revert left as it is now ("Sklon 30°"), for a note after it; null when it put everything back. */
export function keptNote(c: AiSettingsChange, current?: AppSettings | null): string | null {
  const kept = c.kept ?? [];
  if (!kept.length) return null;
  // "Sklon nových střech 30°": no second colon after the note's own (UX-204).
  const name = (k: string) => (isRoofKey(k) ? t("settings.newRoofsOf", { label: label(k) }) : label(k));
  const list = kept.map((k) => `${name(k)} ${current ? changeValue(k, currentValue(current, k)) : ""}`.trim());
  return t("aiChange.kept", { list: list.join(", ") });
}

/** The newest change the user has not answered (the core's `open_ai_change`). */
export function openAiChange(s: AppSettings | undefined | null): AiSettingsChange | null {
  const list = s?.ai_changes ?? [];
  for (let i = list.length - 1; i >= 0; i--) if (list[i].state === "open") return list[i];
  return null;
}
