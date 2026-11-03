// Per-user settings. App-wide ones (folders, language, theme, roof presets,
// AI follow mode) live in the OS settings file shared with the MCP server
// (crates/tectora-core/src/settings.rs) and are cached in localStorage so the
// first paint is right; view state (layout, reviewed AI changes) stays local.

import { api } from "./api";
import { LOCALES, i18n, setLocale, type Locale } from "./i18n.svelte";
import { applyTheme } from "./theme.svelte";
import type { AppSettingsView, Presets } from "./types";

function read<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key);
    return raw ? { ...fallback, ...JSON.parse(raw) } : fallback;
  } catch {
    return fallback;
  }
}

function write(key: string, value: unknown) {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // Not persisted; the value still applies for this session.
  }
}

export const presets = $state<Presets>(read("tectora.presets", { pitch_deg: 40, eave_height: 3, overhang: 0.5 }));

export function savePresets() {
  write("tectora.presets", $state.snapshot(presets));
}

export const prefs = $state(read("tectora.prefs", { followAi: true, layout: "split" as "split" | "plan" | "3d", libraryView: "list" as "list" | "grid" }));

export function savePrefs() {
  write("tectora.prefs", $state.snapshot(prefs));
}

/** Timestamp of the newest AI change the user accepted, per project. */
const reviewed = $state<Record<string, number>>(read("tectora.reviewed", {}));

export function reviewedAt(id: string): number {
  return reviewed[id] ?? 0;
}

export function markReviewed(id: string, ts: number) {
  reviewed[id] = ts;
  write("tectora.reviewed", $state.snapshot(reviewed));
}

/** The app-wide settings as last read from / written to the settings file. */
export const app = $state<{ view: AppSettingsView | null }>({ view: null });

/** Applies settings from the file to the running UI. */
export function applySettings(view: AppSettingsView) {
  app.view = view;
  const s = view.settings;
  if (s.locale && s.locale !== i18n.locale && LOCALES.some((l) => l.id === s.locale)) setLocale(s.locale as Locale);
  applyTheme(s.theme);
  Object.assign(presets, s.roof_defaults);
  write("tectora.presets", $state.snapshot(presets));
  if (prefs.followAi !== s.follow_ai) {
    prefs.followAi = s.follow_ai;
    savePrefs();
  }
}

/**
 * Loads the settings file. The first time, values chosen before the
 * settings file existed (localStorage) are carried over into it.
 */
export async function loadSettings(): Promise<void> {
  let view = await api.getSettings();
  let migrated = false;
  try {
    migrated = localStorage.getItem("tectora.settingsMigrated") === "1";
  } catch {
    migrated = true;
  }
  if (!migrated) {
    const patch: Record<string, unknown> = {};
    const raw = (k: string) => {
      try {
        return localStorage.getItem(k);
      } catch {
        return null;
      }
    };
    if (!view.settings.locale && raw("tectora.locale")) patch.locale = i18n.locale;
    if (raw("tectora.presets")) patch.roof_defaults = $state.snapshot(presets);
    if (raw("tectora.prefs")) patch.follow_ai = prefs.followAi;
    if (Object.keys(patch).length) view = await api.updateSettings(patch).catch(() => view);
    try {
      localStorage.setItem("tectora.settingsMigrated", "1");
    } catch {
      // Migrates again next time; harmless.
    }
  }
  applySettings(view);
}

/** Saves a partial change; rejects with an error code (`settings.err.<code>`). */
export async function updateSettings(patch: Record<string, unknown>): Promise<AppSettingsView> {
  const view = await api.updateSettings(patch);
  applySettings(view);
  return view;
}
