import { describe, expect, it } from "vitest";
import { overlaps, placeLabels, rotatedBox, underHigherRoof } from "./planlabels";

describe("rotatedBox", () => {
  it("is the label box itself without a turn and swaps sides at 90°", () => {
    expect(rotatedBox(10, 20, 40, 10, 0)).toEqual({ x0: -10, y0: 15, x1: 30, y1: 25 });
    const b = rotatedBox(0, 0, 40, 10, 90);
    expect(b.x1 - b.x0).toBeCloseTo(10, 9);
    expect(b.y1 - b.y0).toBeCloseTo(40, 9);
  });
});

describe("placeLabels", () => {
  it("keeps labels that do not overlap and drops a later one that does (UX-012)", () => {
    const dim = { x0: 0, y0: 0, x1: 40, y1: 12 };
    const tagOverDim = { x0: 20, y0: 5, x1: 70, y1: 30 };
    const tagApart = { x0: 100, y0: 0, x1: 150, y1: 30 };
    expect(placeLabels([dim, tagOverDim, tagApart])).toEqual([true, false, true]);
  });

  it("never keeps two overlapping labels and skips labels not drawn", () => {
    const boxes = Array.from({ length: 30 }, (_, i) => rotatedBox((i % 6) * 25, Math.floor(i / 6) * 9, 40, 14, i * 17));
    const keep = placeLabels([null, ...boxes]);
    expect(keep[0]).toBe(false);
    const kept = boxes.filter((_, i) => keep[i + 1]);
    for (let i = 0; i < kept.length; i++) for (let j = i + 1; j < kept.length; j++) expect(overlaps(kept[i], kept[j])).toBe(false);
  });

  it("lets labels that only touch stay", () => {
    expect(placeLabels([{ x0: 0, y0: 0, x1: 10, y1: 10 }, { x0: 10, y0: 0, x1: 20, y1: 10 }])).toEqual([true, true]);
  });
});

describe("underHigherRoof", () => {
  it("is true only when another roof lies clearly above the face", () => {
    expect(underHigherRoof(3.2, 5)).toBe(true);
    expect(underHigherRoof(3.2, 3.22)).toBe(false);
    expect(underHigherRoof(3.2, null)).toBe(false);
    expect(underHigherRoof(null, 5)).toBe(false);
  });
});
