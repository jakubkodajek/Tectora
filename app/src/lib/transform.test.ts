import { describe, expect, it } from "vitest";
import { emptyProject, type EdgeDef, type Point2, type RoofPart } from "./types";
import { mapPart, mirror, offsetWall, regularPolygon, rotation, transformPart, translation } from "./transform";

const wall = (x: number, y: number, pitch = 40, extra: Partial<EdgeDef> = {}): EdgeDef => ({
  start: { x, y },
  pitch_deg: pitch,
  eave_height: 3,
  overhang: 0.5,
  ...extra,
});

// 10 x 6 counter-clockwise: south eave 30°, east gable, north eave 45°, west eave 40°.
const part = (): RoofPart => ({
  name: "A",
  edges: [wall(0, 0, 30), wall(10, 0, 90), wall(10, 6, 45), wall(0, 6, 40)],
});

/** Each wall as [from, to, pitch]. */
const walls = (p: RoofPart) =>
  p.edges.map((e, i) => [e.start, p.edges[(i + 1) % p.edges.length].start, e.pitch_deg] as [Point2, Point2, number]);

const signedArea = (p: RoofPart) =>
  p.edges.reduce((s, e, i) => {
    const q = p.edges[(i + 1) % p.edges.length].start;
    return s + e.start.x * q.y - q.x * e.start.y;
  }, 0) / 2;

describe("plan maps", () => {
  it("moves every wall by the vector", () => {
    const m = mapPart(part(), translation(2, -1));
    expect(m.edges.map((e) => e.start)).toEqual([
      { x: 2, y: -1 },
      { x: 12, y: -1 },
      { x: 12, y: 5 },
      { x: 2, y: 5 },
    ]);
  });

  it("rotates walls and keeps each pitch on its wall", () => {
    const m = mapPart(part(), rotation({ x: 0, y: 0 }, 90));
    expect(walls(m)[0]).toEqual([{ x: 0, y: 0 }, { x: 0, y: 10 }, 30]);
    expect(signedArea(m)).toBeCloseTo(60);
  });

  it("mirrors into a counter-clockwise outline with each pitch on its mirrored wall", () => {
    const m = mapPart(part(), mirror({ x: 0, y: 0 }, 90)); // at the y axis
    expect(signedArea(m)).toBeCloseTo(60);
    const byPitch = Object.fromEntries(walls(m).map(([a, b, p]) => [p, [a, b]]));
    // South eave (0,0)-(10,0) becomes (0,0)-(-10,0), walked the other way.
    expect(byPitch[30]).toEqual([{ x: -10, y: 0 }, { x: 0, y: 0 }]);
    expect(byPitch[90]).toEqual([{ x: -10, y: 6 }, { x: -10, y: 0 }]);
    expect(byPitch[45]).toEqual([{ x: 0, y: 6 }, { x: -10, y: 6 }]);
    expect(byPitch[40]).toEqual([{ x: 0, y: 0 }, { x: 0, y: 6 }]);
  });

  it("mirroring twice gives the part back", () => {
    const m = mirror({ x: 3, y: 1 }, 30);
    const p = part();
    p.edges[1].direction_rad = 0.7;
    const back = mapPart(mapPart(p, m), m);
    back.edges.forEach((e, i) => {
      expect(e.start.x).toBeCloseTo(p.edges[i].start.x, 2);
      expect(e.start.y).toBeCloseTo(p.edges[i].start.y, 2);
      expect(e.pitch_deg).toBe(p.edges[i].pitch_deg);
    });
    expect(back.edges[1].direction_rad).toBeCloseTo(0.7);
  });
});

describe("transformPart", () => {
  it("copies with things on the part and leaves the rest", () => {
    const pr = emptyProject("x");
    pr.parts.push(part());
    pr.chimneys.push({ corners: [{ x: 4, y: 2 }, { x: 5, y: 2 }, { x: 5, y: 3 }, { x: 4, y: 3 }], height: 9 });
    pr.chimneys.push({ corners: [{ x: 40, y: 2 }, { x: 41, y: 2 }, { x: 41, y: 3 }], height: 9 });
    const added = transformPart(pr, 0, translation(20, 0), 2);
    expect(added).toEqual([1, 2]);
    expect(pr.parts[2].edges[0].start).toEqual({ x: 40, y: 0 });
    expect(pr.chimneys.length).toBe(4);
    expect(pr.chimneys[3].corners[0]).toEqual({ x: 44, y: 2 });
  });

  it("moves windows and dormers lying on the part", () => {
    const pr = emptyProject("x");
    pr.parts.push(part());
    pr.windows = [{ x: 2, y: 1, width: 1, height: 1.2 }];
    transformPart(pr, 0, translation(0, 5));
    expect(pr.windows[0]).toMatchObject({ x: 2, y: 6 });
  });
});

describe("wall helpers", () => {
  it("moves a wall outward and stretches its neighbours", () => {
    const p = part();
    expect(offsetWall(p, 0, 1)).toBe(true);
    expect(p.edges.map((e) => e.start)).toEqual([
      { x: 0, y: -1 },
      { x: 10, y: -1 },
      { x: 10, y: 6 },
      { x: 0, y: 6 },
    ]);
    expect(offsetWall(p, 1, -2)).toBe(true);
    expect(p.edges[1].start).toEqual({ x: 8, y: -1 });
    expect(p.edges[2].start).toEqual({ x: 8, y: 6 });
  });

  it("builds a regular polygon with the given side", () => {
    const pts = regularPolygon({ x: 0, y: 0 }, 6, 4);
    expect(pts.length).toBe(6);
    for (let i = 0; i < 6; i++) {
      const [a, b] = [pts[i], pts[(i + 1) % 6]];
      expect(Math.hypot(b.x - a.x, b.y - a.y)).toBeCloseTo(4, 2);
    }
    // First wall horizontal.
    expect(pts[0].y).toBeCloseTo(pts[1].y, 6);
    expect(pts[1].x).toBeGreaterThan(pts[0].x);
  });
});
