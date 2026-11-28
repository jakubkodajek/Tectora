import { describe, expect, it } from "vitest";
import { aboveRoof, placeFaces, placeFrame, planArea, planLength, slopeArea } from "./objectfacts";
import type { CalcResult, Placement, Point2 } from "./types";

const T40 = Math.tan((40 * Math.PI) / 180);

/** A 10 m wide face rising in +y from an eave at y = 0, z = 3, with a pitch break at y = 4 (steeper below). */
function roof(): CalcResult {
  const zb = 3 + 4 * T40;
  const lower: [number, number, number][] = [[0, 0, 3], [10, 0, 3], [10, 4, zb], [0, 4, zb]];
  const upper: [number, number, number][] = [[0, 4, zb], [10, 4, zb], [10, 7, zb + 3 * 0.5], [0, 7, zb + 3 * 0.5]];
  return {
    parts: [{ name: "", geometry: { faces: [
      { edge: 0, pitch_deg: 40, polygon: lower, area: 0 },
      { edge: 0, pitch_deg: 26.565, polygon: upper, area: 0 },
    ], lines: [], gables: [] } }],
  } as unknown as CalcResult;
}

const P = (x: number, y: number): Point2 => ({ x, y });

/** A placement as edit::position gives it on this roof, for an object whose lowest point is at y = low. */
function placement(low: number, eaveY: number, u0 = 2, u1 = 3): Placement {
  return {
    part: 0, edge: 0, pitch_deg: 40, from_eave: 0, from_eave_plan: low - eaveY, from_left: u0, from_right: 10 - u1,
    eave_line: [P((u0 + u1) / 2, eaveY), P((u0 + u1) / 2, low)],
    left_line: [P(0, low), P(u0, low)],
    right_line: [P(u1, low), P(10, low)],
  };
}

describe("object facts", () => {
  it("areas: a 0.6 m square chimney on 40° takes 0.47 m² of roof, as the results panel deducts", () => {
    const sq = [P(0, 0), P(0.6, 0), P(0.6, 0.6), P(0, 0.6)];
    expect(planArea(sq)).toBeCloseTo(0.36, 9);
    expect(planArea([...sq].reverse())).toBeCloseTo(0.36, 9);
    expect(slopeArea(0.36, 40)).toBeCloseTo(0.47, 2);
    expect(planLength(1.18, 40)).toBeCloseTo(0.904, 3);
  });

  it("height above the roof on the upper and lower side; below the roof is negative", () => {
    const corners = [P(4, 1), P(4.6, 1), P(4.6, 1.6), P(4, 1.6)];
    const zLow = 3 + 1 * T40, zHigh = 3 + 1.6 * T40;
    const a = aboveRoof(roof(), corners, 6.5)!;
    expect(a.up).toBeCloseTo(6.5 - zHigh, 2);
    expect(a.down).toBeCloseTo(6.5 - zLow, 2);
    expect(aboveRoof(roof(), corners, 0)!.up).toBeLessThan(0);
    // A corner off the roof: no figure rather than a wrong one.
    expect(aboveRoof(roof(), [P(-1, 1), P(1, 1), P(1, 2)], 6.5)).toBeNull();
    expect(aboveRoof(null, corners, 6.5)).toBeNull();
  });

  it("frame: up the slope and right as seen looking up from the eave, on a front and a back face", () => {
    const front = placeFrame(placement(1, 0));
    expect(front.up.x).toBeCloseTo(0);
    expect(front.up.y).toBeCloseTo(1);
    expect(front.right.x).toBeCloseTo(1);
    // A back face (eave at the top of the plan): up is -y, so right is -x, not mirrored.
    const back: Placement = { ...placement(1, 0), eave_line: [P(5, 8), P(5, 6)], left_line: [P(10, 6), P(8, 6)], right_line: [P(7, 6), P(0, 6)] };
    const f = placeFrame(back);
    expect(f.up.y).toBeCloseTo(-1);
    expect(f.right.x).toBeCloseTo(-1);
    // An object standing on the eave (no eave line): the side lines give the frame.
    const onEave: Placement = { ...placement(0, 0), eave_line: [P(2.5, 0), P(2.5, 0)] };
    expect(placeFrame(onEave).up.y).toBeCloseTo(1);
  });

  it("host face: below the pitch break the distance starts at the eave, above it at the break", () => {
    const below = placeFaces(roof(), placement(1, 0))!;
    expect(below.faces).toHaveLength(2);
    expect(below.host).toBe(0);
    expect(below.hostIsEave).toBe(true);
    const above = placeFaces(roof(), placement(5, 4))!;
    expect(above.host).toBe(1);
    expect(above.hostIsEave).toBe(false);
    expect(placeFaces(roof(), { ...placement(1, 0), edge: 3 })).toBeNull();
  });
});
