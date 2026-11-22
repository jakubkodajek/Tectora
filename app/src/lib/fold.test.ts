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
