import { describe, expect, it } from "vitest";
import { i18n } from "./i18n.svelte";
import { OVERHANG, WALL_HEIGHT, within } from "./limits";

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
