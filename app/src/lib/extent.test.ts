/// <reference types="vite/client" />
import { describe, expect, it } from "vitest";
import { maxOf, minOf } from "./extent";
import { chimneyTop, gableShownByWall, planBox } from "./scene3d";

const N = 200_000;

describe("minOf / maxOf (ARCH-402)", () => {
  it("behave as Math.min / Math.max", () => {
    expect(minOf([3, -1, 2])).toBe(-1);
    expect(maxOf([3, -1, 2])).toBe(3);
    expect(minOf([])).toBe(Infinity);
    expect(maxOf([])).toBe(-Infinity);
    expect(minOf([1, NaN, 0])).toBeNaN();
    expect(maxOf([{ x: 4 }, { x: 9 }], (p) => p.x)).toBe(9);
  });

  it("take 200 000 values, where a spread throws", () => {
    const xs = Array.from({ length: N }, (_, i) => Math.sin(i) * 1000);
    expect(() => Math.max(...Array.from({ length: 2_000_000 }, () => 1))).toThrow(RangeError);
    expect(minOf(xs)).toBeCloseTo(-1000, 0);
    expect(maxOf(xs)).toBeCloseTo(1000, 0);
  });
});

describe("3D helpers with 200 000 points (ARCH-402)", () => {
  it("planBox, chimneyTop and gableShownByWall do not throw", () => {
    const poly = Array.from({ length: N }, (_, i) => [Math.cos(i), Math.sin(i), 3] as [number, number, number]);
    expect(planBox([poly])).toMatchObject({ minX: expect.any(Number), maxY: expect.any(Number) });
    const corners = Array.from({ length: N }, (_, i) => ({ x: i * 1e-5, y: 0 }));
    expect(chimneyTop({ corners, height: 0 } as never, () => 2)).toEqual({ bottom: 1.7, top: 3 });
    expect(() => gableShownByWall(poly, [poly[0], [5, 5, 3]], { x: 0, y: 1 }, 0.5)).not.toThrow();
  });
});

const sources = import.meta.glob(["../**/*.svelte", "../**/*.ts", "!../**/*.test.ts"], { query: "?raw", import: "default", eager: true }) as Record<string, string>;

describe("no spread of a list into Math.min / Math.max in the UI (ARCH-402)", () => {
  it("reads the UI sources", () => {
    expect(Object.keys(sources).some((f) => f.endsWith("scene3d.ts"))).toBe(true);
    expect(Object.keys(sources).some((f) => f.endsWith("Editor.svelte"))).toBe(true);
  });

  it("app/src uses minOf / maxOf for lists", () => {
    const bad: string[] = [];
    for (const [file, text] of Object.entries(sources)) {
      text.split("\n").forEach((line, i) => /Math\.(min|max)\(\s*\.\.\./.test(line) && bad.push(`${file}:${i + 1}`));
    }
    expect(bad).toEqual([]);
  });
});
