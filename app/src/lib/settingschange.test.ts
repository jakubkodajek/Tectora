// UX-003: an AI change of the settings is shown with its values before and
// after, in the user's language, and the pill does not call it a job.
import { describe, expect, it } from "vitest";
import { DICTS, i18n } from "./i18n.svelte";
import { pillText } from "./live.svelte";
import { changeRows, openAiChange } from "./settingschange";
import type { AiSettingsChange, AppSettings } from "./types";

const change = (over: Partial<AiSettingsChange> = {}): AiSettingsChange => ({
  id: 1,
  by: "Claude",
  summary: "Company defaults",
  before: { locale: null, "roof_defaults.overhang": 0.5, "roof_defaults.pitch_deg": 40, follow_ai: true, theme: "system" },
  after: { locale: "de", "roof_defaults.overhang": 0.9, "roof_defaults.pitch_deg": 25, follow_ai: false, theme: "dark" },
  state: "open",
  ...over,
});

describe("AI settings change (UX-003)", () => {
  it("lists every changed value before and after", () => {
    i18n.locale = "cs";
    expect(changeRows(change())).toEqual([
      { key: "locale", label: "Jazyk", before: "Podle systému", after: "Deutsch" },
      { key: "roof_defaults.overhang", label: "Přesah", before: "0,50 m", after: "0,90 m" },
      { key: "roof_defaults.pitch_deg", label: "Sklon", before: "40°", after: "25°" },
      { key: "follow_ai", label: "Sledovat AI", before: "zapnuto", after: "vypnuto" },
      { key: "theme", label: "Motiv", before: "Podle systému", after: "Tmavý" },
    ]);
    i18n.locale = "de";
    const rows = changeRows(change());
    expect(rows[1]).toEqual({ key: "roof_defaults.overhang", label: "Überstand", before: "0,50 m", after: "0,90 m" });
    expect(rows[3].after).toBe(DICTS.de["aiChange.off"]);
    i18n.locale = "en";
    expect(changeRows(change())[2]).toMatchObject({ before: "40°", after: "25°" });
    i18n.locale = "cs";
  });

  it("finds the newest change still waiting for the user", () => {
    const s = (list: AiSettingsChange[]) => ({ ai_changes: list }) as unknown as AppSettings;
    expect(openAiChange(null)).toBeNull();
    expect(openAiChange(s([]))).toBeNull();
    expect(openAiChange(s([change({ id: 1 }), change({ id: 2 }), change({ id: 3, state: "reverted" })]))?.id).toBe(2);
    expect(openAiChange(s([change({ state: "accepted" })]))).toBeNull();
  });

  it("has its texts in all four languages", () => {
    const keys = Object.keys(DICTS.cs).filter((k) => k.startsWith("aiChange."));
    expect(keys.length).toBeGreaterThanOrEqual(7);
    for (const d of Object.values(DICTS)) for (const k of keys) expect(d[k], k).toBeTruthy();
  });

  it("the pill names the settings, not 'another job:' with no name", () => {
    const st = (kind: string, project_id: string | null, name?: string) => ({ ts_ms: 0, actor: "claude", message: "m", project_id, kind, name });
    i18n.locale = "cs";
    expect(pillText(st("settings", null))).toBe("Mění nastavení Tectory");
    expect(pillText(st("reading", "b", "Sedlo"))).toBe("jiná zakázka: Sedlo");
    expect(pillText(st("reading", "b"))).toBe(DICTS.cs["ai.otherProject"]);
    for (const l of ["sk", "en", "de"] as const) {
      i18n.locale = l;
      expect(pillText(st("settings", null))).toBe(DICTS[l]["ai.live.settings"]);
    }
    i18n.locale = "cs";
  });
});
