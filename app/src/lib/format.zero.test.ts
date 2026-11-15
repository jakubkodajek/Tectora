import { describe, expect, it } from "vitest";
import { m2, num } from "./format";
import { i18n } from "./i18n.svelte";

describe("figures that round to zero (audit HUNTC-004)", () => {
  it("shows no minus sign for an area a hair below zero", () => {
    i18n.locale = "cs";
    expect(num(-0.0000209)).toBe("0,00");
    expect(m2(-0)).toBe("0,00 m²");
    expect(num(-0.02)).toBe("-0,02");
    expect(num(152.734)).toBe("152,73");
  });
});
