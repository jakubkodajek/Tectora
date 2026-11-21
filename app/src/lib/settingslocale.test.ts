// TEST-106 (UX-003): when a reverted AI change takes the language back to
// "follow the system", the running UI goes back to the system language
// instead of staying in the language the AI chose.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("./api", () => ({ api: {} }));
vi.mock("./theme.svelte", () => ({ applyTheme: () => {} }));

import { i18n, systemLocale } from "./i18n.svelte";
import { app, applySettings } from "./settings.svelte";
import type { AppSettingsView } from "./types";

const view = (locale: string | null): AppSettingsView => ({
  settings: {
    projects_dir: null,
    drawings_dir: null,
    locale,
    theme: "system",
    roof_defaults: { pitch_deg: 40, eave_height: 3, overhang: 0.5 },
    follow_ai: true,
  },
  effective: {
    projects_dir: "/jobs",
    drawings_dir: "/inbox",
    projects_dir_from_env: false,
    default_projects_dir: "/jobs",
    settings_file: "/settings.json",
  },
});

describe("the language from the settings file", () => {
  // setLocale writes the page's lang; the tests run without a DOM.
  beforeEach(() => vi.stubGlobal("document", { documentElement: { lang: "" } }));
  afterEach(() => {
    vi.unstubAllGlobals();
    app.view = null;
  });

  it("goes back to the system language when the file's language is cleared", () => {
    vi.stubGlobal("navigator", { language: "sk-SK" });
    expect(systemLocale()).toBe("sk");
    applySettings(view("de"));
    expect(i18n.locale).toBe("de");
    applySettings(view(null));
    expect(i18n.locale).toBe("sk");
    expect(document.documentElement.lang).toBe("sk");
  });

  it("keeps the language the user picked while the file never had one", () => {
    vi.stubGlobal("navigator", { language: "sk-SK" });
    i18n.locale = "en";
    applySettings(view(null));
    expect(i18n.locale).toBe("en");
  });
});
