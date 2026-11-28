// The editor's plain outline of a tower the core did not calculate lies
// where the core's footprint would (`tower::Tower::outline`, audit ARCH-006).
import { describe, expect, it } from "vitest";
import { roughFootprint } from "./towers";
import type { Tower } from "./types";

const tower = (over: Partial<Tower>): Tower => ({ center: { x: 0, y: 0 }, width: 2, base_height: 5, tiers: [{ rise: 1, top_width: 0 }], ...over });

/** The core's polygon corners: a = -π/2 + π/n (2i - 1), r = w / 2 / cos(π/n). */
const core = (n: number, w: number) =>
  Array.from({ length: n }, (_, i) => {
    const a = -Math.PI / 2 + (Math.PI / n) * (2 * i - 1);
    const r = w / 2 / Math.cos(Math.PI / n);
    return { x: r * Math.cos(a), y: r * Math.sin(a) };
  });

describe("rough tower footprint", () => {
  it.each([3, 5, 6, 7, 8, 12])("a %i-sided polygon has the core's corners", (n) => {
    const fp = roughFootprint(tower({ shape: "polygon", sides: n }));
    const want = core(n, 2);
    expect(fp).toHaveLength(n);
    fp.forEach((p, i) => {
      expect(p.x).toBeCloseTo(want[i].x, 9);
      expect(p.y).toBeCloseTo(want[i].y, 9);
    });
  });

  it("a triangle has a side parallel to the x axis below the centre", () => {
    const fp = roughFootprint(tower({ shape: "polygon", sides: 3, center: { x: 4, y: 2 } }));
    expect(fp[0].y).toBeCloseTo(fp[1].y, 9);
    expect(fp[0].y).toBeCloseTo(2 - 1, 9);
  });

  it("a circle starts at -90°, as the core's", () => {
    const fp = roughFootprint(tower({ shape: "circle" }));
    expect(fp[0].x).toBeCloseTo(0, 9);
    expect(fp[0].y).toBeCloseTo(-1, 9);
  });
});
