import { beforeEach, describe, expect, it } from "vitest";
import { i18n, LOCALES } from "./i18n.svelte";
import { legend, lineName, lineNames, NAMED_CODES } from "./linecodes";

beforeEach(() => {
  i18n.locale = "cs";
});

describe("line code names (UX-013)", () => {
  it("names the front, side and back wall abutments apart in every language", () => {
    for (const { id } of LOCALES) {
      i18n.locale = id;
      const names = ["WV", "WS", "WH", "W"].map(lineName);
      expect(new Set(names).size, id).toBe(4);
    }
  });

  it("gives every named code its own name, and the sheet all of them", () => {
    for (const { id } of LOCALES) {
      i18n.locale = id;
      const names = lineNames();
      expect(Object.keys(names)).toEqual(NAMED_CODES);
      expect(new Set(Object.values(names)).size, id).toBe(NAMED_CODES.length);
    }
    i18n.locale = "cs";
    expect(lineName("Q")).toBe("Jiná čára");
  });

  it("names every shown row and the sub-codes the roof has, and no others", () => {
    const text = legend(["T", "F", "O", "W"], { T: 40, F: 10, OL: 13, OR: 13, WV: 2.6, WS: 4.72 });
    expect(text).toBe(
      "Kódy linií: T Okapy, F Hřebeny, O Štítové hrany (OL Štítová hrana vlevo, OR Štítová hrana vpravo), W Napojení na zeď (WV Napojení čelní, WS Napojení boční).",
    );
    expect(text).not.toContain("WH");
    expect(text).not.toContain("OS");
  });
});

describe("a hand-drawn line's code from the job file (TEST-1403)", () => {
  it("reads a prototype name as other line, never Object.prototype's member", () => {
    for (const code of ["constructor", "toString", "__proto__", "valueOf"]) expect(lineName(code), code).toBe(lineName("ZZ"));
  });
});
