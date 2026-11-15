import { describe, expect, it } from "vitest";
import { angledStep, draftWalls, lastStep, stepAngle } from "./drawstep";

describe("angledStep (UX-007)", () => {
  it("turns a length and an absolute angle into the next corner, anticlockwise from the right", () => {
    expect(angledStep(10, 0)).toEqual({ dx: 10, dy: 0 });
    expect(angledStep(10, 90)).toEqual({ dx: 0, dy: 10 });
    expect(angledStep(10, 180)).toEqual({ dx: -10, dy: 0 });
    expect(angledStep(10, -90)).toEqual({ dx: 0, dy: -10 });
    expect(angledStep(10, 30)).toEqual({ dx: 8.66, dy: 5 });
    expect(angledStep(4, 135)).toEqual({ dx: -2.828, dy: 2.828 });
  });

  it("measures the angle from the previous wall when it is given", () => {
    // Previous wall to the right; a 100° turn to the left.
    expect(angledStep(5, 100, { dx: 3, dy: 0 })).toEqual(angledStep(5, 100));
    // Previous wall up: +90° turns left, -90° turns right.
    expect(angledStep(6, 90, { dx: 0, dy: 2 })).toEqual({ dx: -6, dy: 0 });
    expect(angledStep(6, -90, { dx: 0, dy: 2 })).toEqual({ dx: 6, dy: 0 });
    // A previous wall of no length counts as none.
    expect(angledStep(6, 90, { dx: 0, dy: 0 })).toEqual({ dx: 0, dy: 6 });
  });

  it("refuses what the fields refuse and never returns NaN or -0", () => {
    expect(angledStep(0, 10)).toBeNull();
    expect(angledStep(-1, 10)).toBeNull();
    expect(angledStep(5, 361)).toBeNull();
    expect(angledStep(NaN, 0)).toBeNull();
    expect(angledStep(5, Infinity)).toBeNull();
    const s = angledStep(5, 90)!;
    expect(Object.is(s.dx, -0)).toBe(false);
  });
});

describe("stepAngle", () => {
  it("is the direction in [0, 360)", () => {
    expect(stepAngle({ dx: 1, dy: 0 })).toBe(0);
    expect(stepAngle({ dx: 0, dy: -1 })).toBe(270);
    expect(stepAngle({ dx: -1, dy: 1 })).toBe(135);
  });
});

describe("lastStep and draftWalls (UX-014)", () => {
  const pts = [
    { x: 0, y: 0 },
    { x: 10, y: 0 },
    { x: 10, y: 0 },
    { x: 10, y: 7.5 },
  ];
  it("lists every wall drawn so far with its length, without walls of no length", () => {
    expect(draftWalls(pts).map((w) => w.length)).toEqual([10, 7.5]);
    expect(draftWalls([pts[0]])).toEqual([]);
    expect(draftWalls(null)).toEqual([]);
  });
  it("gives the last wall, or null before the second corner", () => {
    expect(lastStep(pts)).toEqual({ dx: 0, dy: 7.5 });
    expect(lastStep([pts[0]])).toBeNull();
  });
});
