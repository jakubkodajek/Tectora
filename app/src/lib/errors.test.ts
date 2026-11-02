import { describe, expect, it } from "vitest";
import { describeCalcError, describeError, describeProblem, toAppError } from "./errors";
import { DICTS, i18n, type Locale } from "./i18n.svelte";

const setLocale = (l: Locale) => (i18n.locale = l);
import { emptyProject } from "./types";

describe("translations", () => {
  it("every locale has every key", () => {
    const cs = Object.keys(DICTS.cs);
    for (const [loc, dict] of Object.entries(DICTS)) {
      const missing = cs.filter((k) => !(k in dict));
      expect(missing, loc).toEqual([]);
    }
  });
});

describe("describeError", () => {
  it("translates a backend error code and keeps the detail", () => {
    setLocale("en");
    const d = describeError({ code: "not_found", id: "dum", detail: "dum: project not found" }, "load");
    expect(d.title).toBe("Could not open the design");
    expect(d.message).toContain("no longer in the folder");
    expect(d.detail).toBe("dum: project not found");
  });

  it("recognises Tauri's argument errors as an empty field", () => {
    const e = toAppError("invalid args `project` for command `calc`: invalid type: null, expected f64");
    expect(e.code).toBe("invalid_input");
  });

  it("falls back to a generic message for unknown codes and plain errors", () => {
    setLocale("cs");
    expect(describeError({ code: "something_new" }).message).toBe("Nastala neočekávaná chyba.");
    const d = describeError(new Error("boom"));
    expect(d.code).toBe("unknown");
    expect(d.detail).toBe("boom");
  });
});

describe("calculation errors", () => {
  it("explains DACAD's error and keeps its original text", () => {
    setLocale("de");
    const d = describeCalcError({
      kind: "open_faces",
      code: "E00002",
      message_id: 0xa0f,
      message: "E00002 : Nicht geschlossene Fläche(n) vorhanden",
    });
    expect(d.title).toContain("Flächen sind nicht geschlossen");
    expect(d.original).toBe("E00002: Nicht geschlossene Fläche(n) vorhanden");
  });

  it("names the part and wall of an input problem", () => {
    setLocale("cs");
    const p = emptyProject("x");
    p.parts = [
      { name: "Dům", edges: [] },
      { name: "Garáž", edges: [] },
    ] as never;
    expect(describeProblem({ item: "part", index: 1, edge: 2, code: "pitch_range" }, p)).toBe(
      "Část 2 „Garáž“, stěna 3: sklon musí být větší než 0° a menší než 180°",
    );
  });
});
