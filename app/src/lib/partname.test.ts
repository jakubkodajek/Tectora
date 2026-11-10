import { describe, expect, it } from "vitest";
import { i18n, type Locale } from "./i18n.svelte";
import { isDefaultPartName, partLabel } from "./partname";

const setLocale = (l: Locale) => (i18n.locale = l);

describe("partLabel", () => {
  it("localizes the core's untouched default names by position", () => {
    setLocale("cs");
    expect(partLabel("Main", 0)).toBe("Budova 1");
    expect(partLabel("Main (2)", 1)).toBe("Budova 2");
    expect(partLabel("Part 3", 2)).toBe("Budova 3");
    expect(partLabel("Part 2.1", 4)).toBe("Budova 5");
    expect(partLabel("", 1)).toBe("Budova 2");
    setLocale("de");
    expect(partLabel("Main", 0)).toBe("Gebäude 1");
    setLocale("cs");
  });

  it("keeps names someone typed", () => {
    expect(partLabel("Garáž", 1)).toBe("Garáž");
    expect(partLabel("Mainstreet", 0)).toBe("Mainstreet");
    expect(partLabel("Hlavní střecha", 0)).toBe("Hlavní střecha");
    expect(isDefaultPartName("Part two")).toBe(false);
  });
});
