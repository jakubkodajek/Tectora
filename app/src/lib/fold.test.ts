import { describe, expect, it } from "vitest";
import { fold, searchWords } from "./fold";

describe("fold (the core's library::fold)", () => {
  it("drops accents and case", () => {
    expect(fold("Řezníček Žluťoučký")).toBe("reznicek zlutoucky");
    expect(fold("ÄÖÜ äöü")).toBe("aou aou");
  });
  it("reads full-width letters and digits as plain ones", () => {
    expect(fold("ＴＥＲＲＡＮ １２")).toBe("terran 12");
    expect(fold("Ｎｏｖáｋ")).toBe("novak");
  });
  it("spells out ligatures and letters without a decomposition", () => {
    expect(fold("Straße")).toBe("strasse");
    expect(fold("ﬁnal ﬂat")).toBe("final flat");
    expect(fold("Łódź Øresund Æble")).toBe("lodz oresund aeble");
  });
  it("folds Greek accents and the final sigma", () => {
    expect(fold("Άρης")).toBe(fold("αρησ"));
  });
});

// The same cases the core's `library::fold` is tested with
// (crates/tectora-core/src/library.rs `fold_matches_the_shared_cases`), so
// the library search and MCP `list_projects` find the same jobs (UX-503).
describe("fold follows the core's fold (shared cases)", () => {
  const files = import.meta.glob("../../../tests/fixtures/parity/fold_cases.json", { eager: true, import: "default" }) as Record<string, unknown>;
  const file = Object.values(files)[0] as { cases: [string, string][] };
  it("has the cases", () => expect(file.cases.length).toBeGreaterThan(10));
  for (const [input, folded] of file.cases) {
    it(`folds ${JSON.stringify(input)}`, () => expect(fold(input)).toBe(folded));
  }
  it("keeps compatibility characters as the core does", () => {
    expect(fold("Plocha 120 m²")).not.toContain("m2");
    expect(fold("Ⅻ etapa")).not.toContain("xii");
  });
});

// Audit 2026-11-22 HUNT-602 / TEST-608: the core's fold over every code
// point U+0020 to U+2FFFF and many letter + mark sequences, written by the
// core test `library::tests::fold_matches_the_generated_parity_file`
// (`changed`: the inputs it changes; every other input folds to itself).
// The inputs are made here as the core makes them.
describe("fold equals the core's fold over every code point", () => {
  const files = import.meta.glob("../../../tests/fixtures/parity/fold_all.json", { eager: true, import: "default" }) as Record<string, unknown>;
  const file = Object.values(files)[0] as { changed: Record<string, string>; words: [string, string[]][] };
  const nfcFiles = import.meta.glob("../../../tests/fixtures/parity/nfc_inputs.json", { eager: true, import: "default" }) as Record<string, unknown>;
  const nfcInputs = Object.values(nfcFiles)[0] as { inputs: string[] };
  const inputs = (): string[] => {
    const out: string[] = [];
    for (let c = 0x20; c < 0x30000; c++) if (c < 0xd800 || c >= 0xe000) out.push(String.fromCodePoint(c));
    const marks = Array.from({ length: 0x70 }, (_, i) => String.fromCodePoint(0x300 + i));
    for (const l of "aeiouyAEIOUYcnszrdtlgkhwjCNSZRDTLGKHWJαεηιουωΑΕΗΙΟΥΩæøÆØ") for (const m of marks) out.push(l + m);
    for (const l of "aeoAEOuUαω")
      for (const m of marks) for (const m2 of [0x301, 0x302, 0x303, 0x308, 0x313, 0x323, 0x328, 0x342]) out.push(l + m + String.fromCodePoint(m2));
    for (const l of "aAo")
      for (const m of [0x1ab0, 0x1dc0, 0x20d0, 0xfe20, 0x0483, 0x0591, 0x064b, 0x093c, 0x302a, 0x3099]) out.push(l + String.fromCodePoint(m));
    // TEST-710: decomposed sequences with marks outside U+0300-036F and
    // Hangul jamo, as the core's `parity_inputs`.
    out.push(...nfcInputs.inputs);
    const ch = String.fromCodePoint;
    for (let l = 0x1100; l <= 0x1112; l++)
      for (let v = 0x1161; v <= 0x1175; v++) {
        out.push(ch(l) + ch(v));
        for (const t of [0x11a8, 0x11ab, 0x11af, 0x11c2]) out.push(ch(l) + ch(v) + ch(t));
      }
    for (let t = 0x11a7; t <= 0x11c3; t++) out.push("\uac00" + ch(t), "\uac01" + ch(t));
    return out;
  };
  it("has the file", () => expect(Object.keys(file.changed).length).toBeGreaterThan(1000));
  it("folds every input as the core", () => {
    const hex = (s: string) => [...s].map((c) => "U+" + c.codePointAt(0)!.toString(16).toUpperCase()).join(" ");
    const differ: string[] = [];
    for (const i of inputs()) {
      const want = file.changed[i] ?? i;
      const got = fold(i);
      if (got !== want && differ.length < 20) differ.push(`${hex(i)}: app ${hex(got)}, core ${hex(want)}`);
    }
    expect(differ).toEqual([]);
  });
  it("folds the oxia vowels and the accented ligatures", () => {
    expect(fold("Vila \u1f71lfa")).toBe(fold("vila αlfa"));
    expect(fold("\u01fd \u01e3 \u01ff")).toBe("ae ae o");
  });
  it("splits search words as the core", () => {
    for (const [text, words] of file.words) expect(searchWords(fold(text))).toEqual(words);
  });
});
