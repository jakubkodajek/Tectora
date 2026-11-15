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

  it("never names DACAD in the UI", () => {
    for (const [loc, dict] of Object.entries(DICTS)) {
      const hits = Object.entries(dict).filter(([, v]) => /dacad/i.test(v)).map(([k]) => k);
      expect(hits, loc).toEqual([]);
    }
  });
});

describe("describeError", () => {
  it("translates a backend error code and keeps the detail", () => {
    setLocale("en");
    const d = describeError({ code: "not_found", id: "dum", detail: "dum: project not found" }, "load");
    expect(d.title).toBe("Could not open the job");
    expect(d.message).toContain("no longer in the folder");
    expect(d.detail).toBe("dum: project not found");
  });

  it("too_many names the pitch break and dormer limits and points to the details (DR-004)", () => {
    for (const l of ["cs", "sk", "en", "de"] as Locale[]) {
      setLocale(l);
      const d = describeError({ code: "too_many", detail: "too many pitch breaks of one wall: 65 (at most 64)" }, "load");
      expect(d.message, l).toContain("64");
      const text = d.message.toLocaleLowerCase(l);
      expect(text, l).toContain(DICTS[l]["tool.dormer"].toLocaleLowerCase(l).slice(0, 4));
      expect(text, l).toContain(DICTS[l]["err.details"].toLocaleLowerCase(l).slice(0, 6));
      expect(d.detail).toBe("too many pitch breaks of one wall: 65 (at most 64)");
    }
  });

  it("not_calculated says the result cannot be stored for the offer", () => {
    for (const l of ["cs", "sk", "en", "de"] as Locale[]) {
      setLocale(l);
      const d = describeError({ code: "not_calculated" }, "save");
      expect(d.message, l).toBe(DICTS[l]["err.not_calculated"]);
      expect(d.message, l).not.toBe(DICTS[l]["err.unknown"]);
    }
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
  it("explains the calculation error in the user's language only", () => {
    setLocale("de");
    const d = describeCalcError({
      kind: "open_faces",
      code: "E00002",
      message_id: 0xa0f,
      message: "E00002 : Nicht geschlossene Fläche(n) vorhanden",
    });
    expect(d.title).toContain("Flächen sind nicht geschlossen");
    expect(d.code).toBe("open_faces");
    expect(JSON.stringify(d)).not.toContain("DACAD");
  });

  it("names the building whose roof fails on its own", () => {
    // One core error stops every roof of the job; the bar says which building to fix.
    setLocale("cs");
    const p = emptyProject("x");
    p.parts = [
      { name: "Main", edges: [] },
      { name: "Garáž", edges: [] },
    ] as never;
    const e = { kind: "open_faces", message: "x", parts: [1] } as const;
    const d = describeCalcError({ ...e, parts: [1] }, p);
    expect(d.title.startsWith("Budova 2 „Garáž“ – ")).toBe(true);
    expect(d.hint).toContain("Ostatní budovy");
    // One building, or no culprit: as before.
    expect(describeCalcError({ ...e, parts: [] }, p).title).toBe("Střechu nelze spočítat: plochy se neuzavírají");
  });

  it("adds the parallel walls to the kind's own hint, numbered from 1 (TEST-007, ARCH-006)", () => {
    for (const l of ["cs", "sk", "en", "de"] as Locale[]) {
      setLocale(l);
      const plain = describeCalcError({ kind: "cut", message: "x" });
      expect(plain.hint, l).toBe(DICTS[l]["calc.hint.cut"]);
      const d = describeCalcError({ kind: "cut", message: "x", edges: [0, 2] });
      // The kind's hint stays, the walls follow it.
      expect(d.hint.startsWith(DICTS[l]["calc.hint.cut"] + " "), l).toBe(true);
      const walls = d.hint.slice(DICTS[l]["calc.hint.cut"].length);
      expect(walls, l).toMatch(/\b1\b/);
      expect(walls, l).toMatch(/\b3\b/);
      expect(walls, l).not.toMatch(/\b[02]\b/);
      expect(walls, l).not.toContain("{");
      // One wall is no pair: the kind's hint alone.
      expect(describeCalcError({ kind: "cut", message: "x", edges: [4] }).hint, l).toBe(DICTS[l]["calc.hint.cut"]);
      expect(describeCalcError({ kind: "cut", message: "x", edges: [] }).hint, l).toBe(DICTS[l]["calc.hint.cut"]);
    }
    // With several buildings the three sentences add up.
    setLocale("cs");
    const p = emptyProject("x");
    p.parts = [
      { name: "Main", edges: [] },
      { name: "Garáž", edges: [] },
    ] as never;
    const d = describeCalcError({ kind: "open_faces", message: "x", parts: [0], edges: [0, 2] }, p);
    expect(d.hint).toContain(DICTS.cs["calc.hint.open_faces"]);
    expect(d.hint).toContain("Roviny stěn 1 a 3");
    expect(d.hint).toContain("Ostatní budovy");
  });

  it("names the part and wall of an input problem", () => {
    setLocale("cs");
    const p = emptyProject("x");
    p.parts = [
      { name: "Dům", edges: [] },
      { name: "Garáž", edges: [] },
    ] as never;
    expect(describeProblem({ item: "part", index: 1, edge: 2, code: "pitch_range" }, p)).toBe(
      "Budova 2 „Garáž“, stěna 3: sklon musí být od 2° do 178°",
    );
  });
});
