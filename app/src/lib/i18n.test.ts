import { describe, expect, it } from "vitest";
import { DICTS } from "./i18n.svelte";

const placeholders = (s: string) => [...s.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort();

// Roofing words from the German locale that must not leak into the others.
const GERMAN = /\b(Nicht|Fläche|Dach|Traufe|Gaube|Kehle|Grat|Ortgang|Knick|Schlucht|Richtungswechsel|Aufbaudicke)\b/;

describe("translations", () => {
  const keys = Object.keys(DICTS.cs);

  it("every locale has exactly the Czech keys", () => {
    for (const [loc, dict] of Object.entries(DICTS)) {
      expect(keys.filter((k) => !(k in dict)), `${loc} missing`).toEqual([]);
      expect(Object.keys(dict).filter((k) => !(k in DICTS.cs)), `${loc} extra`).toEqual([]);
    }
  });

  it("every translation uses the same placeholders as Czech", () => {
    for (const [loc, dict] of Object.entries(DICTS)) {
      const bad = keys.filter((k) => placeholders(dict[k]).join() !== placeholders(DICTS.cs[k]).join());
      expect(bad, loc).toEqual([]);
    }
  });

  it("has no empty strings", () => {
    for (const [loc, dict] of Object.entries(DICTS)) {
      expect(Object.entries(dict).filter(([, v]) => !v.trim()).map(([k]) => k), loc).toEqual([]);
    }
  });

  it("never names DACAD in the UI", () => {
    for (const [loc, dict] of Object.entries(DICTS)) {
      expect(Object.entries(dict).filter(([, v]) => /dacad/i.test(v)).map(([k]) => k), loc).toEqual([]);
    }
  });

  it("shows German roofing words only in the German locale", () => {
    for (const [loc, dict] of Object.entries(DICTS)) {
      if (loc === "de") continue;
      expect(Object.entries(dict).filter(([, v]) => GERMAN.test(v)).map(([k]) => k), loc).toEqual([]);
    }
  });
});
