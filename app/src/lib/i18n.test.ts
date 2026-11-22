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

  // Glossary: a chimney is "Schornstein" in German, never "Kamin" (UX-007).
  it("names a chimney Schornstein in German, never Kamin", () => {
    expect(Object.entries(DICTS.de).filter(([, v]) => /Kamin/.test(v)).map(([k]) => k)).toEqual([]);
  });
});

// Edge-cases class 4 (tools that look alike, TEST-012): "Building" draws a
// building with a calculated roof, "Manual face" adds one face with a pitch,
// "Flat roof" switches the whole job. A roofer must tell them apart by label
// and hint in every language.
describe("tools that look alike", () => {
  const tools = ["tool.draw", "tool.face", "tool.flat", "tool.standard", "tool.line"] as const;
  it("building, manual face and flat roof have distinct labels and hints in every language", () => {
    for (const [loc, d] of Object.entries(DICTS)) {
      const labels = tools.map((k) => d[k].trim().toLocaleLowerCase(loc));
      expect(new Set(labels).size, `${loc} labels ${labels.join(" | ")}`).toBe(tools.length);
      const hints = tools.map((k) => d[`${k}Hint`].trim().toLocaleLowerCase(loc));
      expect(new Set(hints).size, `${loc} hints`).toBe(tools.length);
      // "Make it a building" is not a second Building tool.
      expect(d["manual.toPart"].toLocaleLowerCase(loc), loc).not.toBe(labels[0]);
      for (const k of tools) expect(d[`${k}Hint`].length, `${loc} ${k}Hint`).toBeGreaterThan(d[k].length);
    }
  });
  it("the manual face hint points to the Building tool by its label", () => {
    for (const [loc, d] of Object.entries(DICTS)) {
      // The Building tool by its own label, so a renamed tool is noticed.
      expect(d["tool.faceHint"], loc).toContain(d["tool.draw"]);
      // And it is not the hint of the tool it points to.
      expect(d["tool.faceHint"], loc).not.toBe(d["tool.drawHint"]);
    }
  });
});

// Round-3 audit UX-204: one word per concept in the round-2 texts.
describe("round-3 wording (UX-204)", () => {
  it("calls a round dormer's segments by the field's word", () => {
    // One word for field and message: "segment", as the refusal always said (HUNT-103).
    for (const loc of ["cs", "sk"] as const) {
      expect(DICTS[loc]["round.segments"], loc).toMatch(/segment/);
      expect(DICTS[loc]["dormer.err.segments"], loc).toMatch(/segment/);
      expect(DICTS[loc]["dormer.err.segments"], loc).not.toMatch(/úsek/);
    }
  });
  it("says Verwerfen for the AI card's revert in German throughout", () => {
    expect(DICTS.de["aiChange.state.nothing"]).toMatch(/verwerfen/);
    expect(DICTS.de["aiChange.state.nothing"]).not.toMatch(/zurück/);
  });
  it("does not call the AI folder request a move", () => {
    for (const [loc, d] of Object.entries(DICTS)) expect(d["folderReq.title"], loc).not.toMatch(/Přesun|Presun|Move|verschieben/i);
  });
});

// Round-6 audit UX-504: the limit of a roof's stored figures has its units.
describe("round-6 wording (UX-504)", () => {
  it("names m² for the area and m for the length in err.figures_too_large", () => {
    for (const [loc, d] of Object.entries(DICTS)) {
      expect(d["err.figures_too_large"], loc).toMatch(/1[ .,]000[ .,]000[ .,]000 m²/);
      expect(d["err.figures_too_large"], loc).toMatch(/1[ .,]000[ .,]000[ .,]000 m(?!²)/);
    }
  });
});
