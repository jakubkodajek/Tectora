import { describe, expect, it } from "vitest";
import { fold } from "./fold";

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
