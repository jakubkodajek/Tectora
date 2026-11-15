import { describe, expect, it } from "vitest";
import { i18n } from "./i18n.svelte";
import { COPIES, MIRROR_COPIES, OVERHANG, WALL_HEIGHT, copiesBounds, within } from "./limits";

describe("within (UX-001)", () => {
  it("takes a value inside the bounds", () => {
    expect(within(0.5, OVERHANG)).toBe(true);
    expect(within(0, OVERHANG)).toBe(true);
    expect(within(20, OVERHANG)).toBe(true);
    expect(within(3, WALL_HEIGHT)).toBe(true);
  });

  it("refuses 50 m of overhang with the range, in the UI language", () => {
    i18n.locale = "cs";
    expect(within(50, OVERHANG)).toBe("Povolený rozsah 0 až 20");
    i18n.locale = "en";
    expect(within(50, OVERHANG)).toBe("Allowed range 0 to 20");
    expect(within(-0.1, OVERHANG)).toBe("Allowed range 0 to 20");
    expect(within(1500, WALL_HEIGHT)).toBe("Allowed range 0 to 1000");
    expect(within(NaN, OVERHANG)).not.toBe(true);
  });
});

describe("copiesBounds (UI-009)", () => {
  it("allows 50 copies, one for a mirror, and says the range beyond", () => {
    expect(copiesBounds("move")).toEqual({ min: 0, max: 50 });
    expect(copiesBounds("rotate")).toBe(COPIES);
    expect(copiesBounds("mirror")).toBe(MIRROR_COPIES);
    i18n.locale = "en";
    expect(within(120, copiesBounds("move"))).toBe("Allowed range 0 to 50");
    expect(within(3, copiesBounds("mirror"))).toBe("Allowed range 0 to 1");
  });
});
