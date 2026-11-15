import { describe, expect, it } from "vitest";
import { labelsFit } from "./fitlabels";

describe("labelsFit (UI-005)", () => {
  it("keeps labels that fit side by side", () => {
    // cs at 1100 px: Výsledky, Postup, AI, Zakázka in a 274 px tab list.
    expect(labelsFit([52, 42, 14, 50], 26, 268)).toBe(true);
  });

  it("hides them when one would be cut (de: Ergebnisse, Rechenweg, KI, Auftrag)", () => {
    expect(labelsFit([68, 70, 14, 50], 26, 268)).toBe(false);
  });

  it("counts fractional widths up", () => {
    expect(labelsFit([10.2], 0, 10)).toBe(false);
    expect(labelsFit([10], 0, 10)).toBe(true);
  });
});
