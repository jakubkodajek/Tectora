/// <reference types="vite/client" />
// The draw bar's defaults for new roofs (UI-003, UX-004): the fields take
// only what the settings file takes, a refused value never reaches the
// presets the next building is drawn with, and the refusal is a settings
// error, not "the job could not be saved".
import { beforeEach, describe, expect, it, vi } from "vitest";

const updateSettings = vi.fn();
vi.mock("./api", () => ({ api: { updateSettings: (p: unknown) => updateSettings(p) } }));
vi.mock("./theme.svelte", () => ({ applyTheme: () => {} }));

import { EAVE_HEIGHT, PITCH, PRESET_PITCH } from "./limits";
import { presets, setPreset } from "./settings.svelte";

const drawBar = import.meta.glob("./DrawBar.svelte", { query: "?raw", import: "default", eager: true }) as Record<string, string>;

describe("draw bar presets", () => {
  beforeEach(() => {
    updateSettings.mockReset();
    presets.pitch_deg = 40;
    presets.eave_height = 3;
  });

  it("bound the eave at 0 and the pitch at a wall's 2°", () => {
    expect(EAVE_HEIGHT.min).toBe(0);
    expect(PRESET_PITCH.min).toBe(PITCH.min);
    expect(PRESET_PITCH.max).toBe(89);
  });

  it("keeps the old preset when the settings refuse the new one", async () => {
    updateSettings.mockRejectedValue({ code: "invalid_value", detail: "invalid_value:pitch_deg" });
    await expect(setPreset("pitch_deg", 1.5)).rejects.toBeTruthy();
    expect(presets.pitch_deg).toBe(40);
    expect(updateSettings).toHaveBeenCalledWith({ roof_defaults: { pitch_deg: 1.5 } });
  });

  it("takes the preset the settings file stored", async () => {
    updateSettings.mockResolvedValue({
      settings: { locale: null, theme: "system", roof_defaults: { pitch_deg: 35, eave_height: 3, overhang: 0.5 }, follow_ai: true },
    });
    await setPreset("pitch_deg", 35);
    expect(presets.pitch_deg).toBe(35);
  });

  it("reports a refusal as a settings error", () => {
    const src = Object.values(drawBar)[0];
    expect(src).toContain('notifyError(e, "settings")');
    expect(src).not.toContain('notifyError(e, "save")');
    expect(src).toContain("PRESET_PITCH.min");
  });
});
