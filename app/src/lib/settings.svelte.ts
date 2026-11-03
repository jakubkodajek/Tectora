// Per-user settings kept in localStorage: roof presets (DACAD
// "Voreinstellungen"), AI follow mode and which AI changes were reviewed.

import type { Presets } from "./types";

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
