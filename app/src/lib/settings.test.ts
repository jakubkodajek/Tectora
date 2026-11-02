// The Settings screen must be fully translated and never name DACAD.
import { describe, expect, it } from "vitest";
import { DICTS } from "./i18n.svelte";

describe("settings translations", () => {
  const keys = Object.keys(DICTS.cs).filter((k) => k.startsWith("settings."));

  it("has the settings keys", () => {
    expect(keys.length).toBeGreaterThan(20);
  });

  for (const locale of ["sk", "en", "de"] as const) {
    it(`${locale} has every settings key`, () => {
      expect(keys.filter((k) => !DICTS[locale][k])).toEqual([]);
    });
  }

  it("no settings text mentions DACAD", () => {
    for (const d of Object.values(DICTS)) {
      for (const k of keys) expect(d[k]).not.toMatch(/dacad/i);
    }
  });
});
