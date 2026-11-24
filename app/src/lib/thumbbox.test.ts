import { describe, expect, it } from "vitest";
import { thumbBox } from "./thumbbox";

describe("plan thumbnail box (audit 2026-11-22 ARCH-402)", () => {
  it("takes 200 000 points without a stack overflow", () => {
    const face: [number, number][] = [];
    for (let i = 0; i < 200_000; i++) face.push([Math.cos(i) * 10 + 5, Math.sin(i) * 4 - 2]);
    // A spread of this many points throws a RangeError.
    expect(() => Math.max(...face.map((p) => p[0]))).toThrow(RangeError);
    const box = thumbBox([face], 0);
    expect(box.x).toBeCloseTo(-5, 3);
    expect(box.y).toBeCloseTo(-6, 3);
    expect(box.w).toBeCloseTo(20, 3);
    expect(box.h).toBeCloseTo(8, 3);
  });

  it("pads by the larger side and has a unit box when there is nothing", () => {
    expect(thumbBox([[[0, 0], [10, 0], [10, 4]]], 0.1)).toEqual({ x: -1, y: -1, w: 12, h: 6 });
    expect(thumbBox([], 0.1)).toEqual({ x: 0, y: 0, w: 1, h: 1 });
    expect(thumbBox([[]], 0.1)).toEqual({ x: 0, y: 0, w: 1, h: 1 });
  });
});
