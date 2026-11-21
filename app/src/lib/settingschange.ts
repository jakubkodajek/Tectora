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

/** One row per changed setting: label, before, after. */
export function changeRows(c: AiSettingsChange): ChangeRow[] {
  return Object.keys(c.after).map((key) => ({
    key,
    label: label(key),
    before: changeValue(key, c.before[key]),
    after: changeValue(key, c.after[key]),
  }));
}

/** The newest change the user has not answered (the core's `open_ai_change`). */
export function openAiChange(s: AppSettings | undefined | null): AiSettingsChange | null {
  const list = s?.ai_changes ?? [];
  for (let i = list.length - 1; i >= 0; i--) if (list[i].state === "open") return list[i];
  return null;
}
