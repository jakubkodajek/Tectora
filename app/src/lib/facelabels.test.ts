import { describe, expect, it } from "vitest";
import { centroid, contourAngle, faceTags } from "./facelabels";
import type { RoofFace } from "./types";

// A 10 x 8 gable roof with the ridge along y at x = 5, eaves at x = 0 and x = 10.
const west: RoofFace = { edge: 0, pitch_deg: 33.7, area: 48.08, polygon: [[0, 0, 3], [5, 0, 6.33], [5, 8, 6.33], [0, 8, 3]] };
const east: RoofFace = { edge: 2, pitch_deg: 33.7, area: 48.08, polygon: [[10, 8, 3], [5, 8, 6.33], [5, 0, 6.33], [10, 0, 3]] };
// A face falling towards -y (eave along x).
const south: RoofFace = { edge: 1, pitch_deg: 45, area: 20, polygon: [[0, 0, 3], [10, 0, 3], [10, 2, 5], [0, 2, 5]] };

describe("centroid", () => {
  it("is the area centroid, not the vertex mean", () => {
    // A triangle with a doubled vertex: the vertex mean is pulled to (0, 0).
    expect(centroid([[0, 0], [0, 0], [3, 0], [0, 3]])).toEqual([1, 1]);
  });

  it("falls back to the vertex mean without area", () => {
    expect(centroid([[0, 0], [2, 0], [4, 0]])).toEqual([2, 0]);
  });
});

describe("contourAngle", () => {
  it("runs along the eave of the face", () => {
    expect(contourAngle(west.polygon)).toBeCloseTo(90);
    expect(contourAngle(east.polygon)).toBeCloseTo(90);
    expect(contourAngle(south.polygon)).toBeCloseTo(0);
  });

  it("keeps the text readable on a diagonal face", () => {
    // Falls towards (-1, -1): the contour runs at -45°, never upside down.
    const a = contourAngle([[0, 0, 0], [2, 0, 2], [0, 2, 2]]);
    expect(a).toBeCloseTo(-45);
  });

  it("is 0 for a flat face", () => {
    expect(contourAngle([[0, 0, 3], [4, 0, 3], [4, 4, 3], [0, 4, 3]])).toBe(0);
  });
});

describe("faceTags", () => {
  it("labels each face with its pitch and area at its centre", () => {
    const [w, e] = faceTags([west, east]);
    expect(w.x).toBeCloseTo(2.5);
    expect(w.y).toBeCloseTo(4);
    expect(e.x).toBeCloseTo(7.5);
    expect(w.pitch_deg).toBe(33.7);
    expect(w.area).toBe(48.08);
    // Along the eave (8 m) and across it (5 m).
    expect(w.along).toBeCloseTo(8);
    expect(w.across).toBeCloseTo(5);
  });

  it("skips faces without area", () => {
    expect(faceTags([{ ...west, area: 0 }, { ...west, polygon: [[0, 0, 0], [1, 0, 0]] }])).toEqual([]);
  });
});
