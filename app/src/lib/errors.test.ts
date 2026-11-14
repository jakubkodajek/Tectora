import { describe, expect, it } from "vitest";
import { describeCalcError, describeError, describeProblem, describeRefusal, refusalReason, toAppError } from "./errors";
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

describe("refusals of the core (ERR-003)", () => {
  const below = "at a building height of 0.500 m an eave would be -0.500 m, below the ground";

  it("a refused building height is a refusal with the core's reason, in every language", () => {
    for (const l of ["cs", "sk", "en", "de"] as Locale[]) {
      setLocale(l);
      const d = describeRefusal({ code: "refused", detail: below }, "height");
      expect(d, l).not.toBeNull();
      expect(d!.title, l).toBe(DICTS[l]["refuse.title.height"]);
      expect(d!.message, l).toBe(DICTS[l]["refuse.belowGround"]);
      expect(d!.message, l).not.toBe(DICTS[l]["err.invalid_input"]);
      expect(d!.title, l).not.toBe(DICTS[l]["err.action.calc"]);
      expect(d!.detail).toBe(below);
      expect(d!.code).toBe("refused");
    }
  });

  it("names the range and the face the core gives", () => {
    setLocale("cs");
    expect(refusalReason("from_eave 600.000 is out of range (0 to 500 m from the edge of its roof face)")).toBe("Povolený rozsah 0 až 500");
    expect(refusalReason("height 1e9 is out of range (0.5 to 1000 m)")).toBe("Povolený rozsah 0,5 až 1000");
    expect(refusalReason("the object would leave its roof face (from the eave); give a smaller distance")).toBe(DICTS.cs["place.refused"]);
    expect(refusalReason("the object stands on no roof face, so it has no distances to set")).toBe(DICTS.cs["refuse.noFace"]);
    expect(refusalReason("hand-drawn face 3 has fewer than 3 corners")).toBe(DICTS.cs["refuse.other"]);
    expect(describeRefusal({ code: "refused", detail: "x" }, "face")!.title).toBe(DICTS.cs["refuse.title.face"]);
  });

  it("other errors are not refusals; the code itself has a message", () => {
    expect(describeRefusal({ code: "out_of_range", detail: "x" }, "height")).toBeNull();
    expect(describeRefusal(new Error("boom"), "place")).toBeNull();
    setLocale("en");
    expect(describeError({ code: "refused" }).message).toBe(DICTS.en["err.refused"]);
  });
});
