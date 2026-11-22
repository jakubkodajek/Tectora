// A job at the limits (200 buildings, check_limits) must not freeze the plan:
// the covered stretches of every roof line and the face labels were worked
// out by testing every sample against every face of every building through
// the reactive result (UI-501, TEST-501). Display only.

import { describe, expect, it } from "vitest";
import { hostHeightAt, hostIndex, hostParts } from "./dormers";
import { crossingIndex, crossings, type Poly3 } from "./scene3d";
import { coveredPieces, visibleFaceTags } from "./planjoin";
import type { CalcResult, RoofFace, RoofLine } from "./types";

type P3 = [number, number, number];

/** A hipped roof over a 10 x 8 m rectangle at (ox, oy), eave at 3 m. */
function hipped(ox: number, oy: number, eave = 3, ridge = 6) {
  const p = (x: number, y: number, z: number): P3 => [ox + x, oy + y, z];
  const [a, b, c, d] = [p(0, 0, eave), p(10, 0, eave), p(10, 8, eave), p(0, 8, eave)];
  const [r1, r2] = [p(4, 4, ridge), p(6, 4, ridge)];
  const face = (edge: number, polygon: P3[]): RoofFace => ({ edge, pitch_deg: 37, polygon, area: 30 });
  const faces = [face(0, [a, b, r2, r1]), face(1, [b, c, r2]), face(2, [c, d, r1, r2]), face(3, [d, a, r1])];
  const line = (kind: RoofLine["kind"], x: P3, y: P3): RoofLine => ({ kind, a: x, b: y });
  const lines = [line("eave", a, b), line("eave", b, c), line("eave", c, d), line("eave", d, a), line("hip", a, r1), line("hip", b, r2), line("hip", c, r2), line("hip", d, r1), line("ridge", r1, r2)];
  return { faces, lines };
}

const q = () => ({ roof_area: 0, ridge_length: 0, hip_length: 0, hip_count: 0, valley_length: 0, valley_count: 0, eave_length: 0, verge_length: 0, pult_length: 0, knick_length: 0, gorge_length: 0, abutment_length: 0, ridge_height: 0 });

function result(parts: { faces: RoofFace[]; lines: RoofLine[] }[]): CalcResult {
  return { parts: parts.map((g, i) => ({ name: `B${i}`, geometry: { ...g, gables: [] }, quantities: q() })), totals: q(), codes: {}, problems: [] };
}

/** 200 buildings 14 m apart in a 20 x 10 grid, as the reviewers' estate. */
const estate = () => result(Array.from({ length: 200 }, (_, i) => hipped((i % 20) * 14, Math.floor(i / 20) * 12)));

describe("a plan of 200 buildings", () => {
  it("splits every roof line by cover and keeps every face label within the time budget", () => {
    const r = estate();
    const t0 = performance.now();
    const top = hostIndex(r);
    const pieces = coveredPieces(r, top);
    const tags = visibleFaceTags(r, top);
    const ms = performance.now() - t0;
    expect(ms).toBeLessThan(500);
    // Buildings 4 m apart cover nothing of each other.
    expect(pieces.flat().every((l) => l.pieces.length === 1 && !l.pieces[0].hidden)).toBe(true);
    expect(tags.length).toBe(200 * 4);
  });
});

describe("hostIndex", () => {
  it("gives the same height as hostHeightAt everywhere, also on face edges and outside", () => {
    // Two overlapping buildings (a higher wing over a lower one) and a dormer face.
    const low = hipped(0, 0, 3, 5);
    const high = hipped(7, 2, 3.5, 7);
    low.faces.push({ edge: Number.MAX_SAFE_INTEGER * 2, pitch_deg: 30, polygon: [[2, 1, 9], [4, 1, 9], [3, 2, 9]], area: 1 });
    const r = result([low, high]);
    const top = hostIndex(r);
    for (let x = -1; x <= 18.5; x += 0.25) {
      for (let y = -1; y <= 11; y += 0.25) {
        expect(top({ x, y })).toBe(hostHeightAt(r, { x, y }));
      }
    }
    expect(hostIndex(null)({ x: 1, y: 1 })).toBeNull();
  });

  it("dashes the lower roof's lines under the higher one and hides its labels there, as before", () => {
    const low = hipped(0, 0, 3, 5);
    const high = hipped(7, 2, 3.5, 7);
    const r = result([low, high]);
    const top = hostIndex(r);
    const slow = (p: { x: number; y: number }) => hostHeightAt(r, p);
    expect(coveredPieces(r, top)).toEqual(coveredPieces(r, slow));
    expect(coveredPieces(r)).toEqual(coveredPieces(r, slow));
    expect(coveredPieces(r).flat().some((l) => l.pieces.some((p) => p.hidden))).toBe(true);
    expect(visibleFaceTags(r, top)).toEqual(visibleFaceTags(r, slow));
  });
});

// TEST-607: the 500 ms budget above also passed an index four times slower
// (no per-building box). Work is counted here, not time.
describe("hostIndex: work per point", () => {
  it("tests a point only against faces whose box holds it: at most the faces of one building in the estate", () => {
    const r = estate();
    const stats = { inside: 0 };
    const top = hostIndex(r, stats);
    let points = 0;
    let worst = 0;
    for (let x = -2; x <= 20 * 14; x += 3.7) {
      for (let y = -2; y <= 10 * 12; y += 3.3) {
        const before = stats.inside;
        top({ x, y });
        worst = Math.max(worst, stats.inside - before);
        points++;
      }
    }
    // Buildings 4 m apart: a point lies in the box of one building's faces at most (4 of them).
    expect(worst).toBeLessThanOrEqual(4);
    expect(stats.inside).toBeLessThanOrEqual(points * 4);
    // And far less than every face for every point (800 faces).
    expect(stats.inside).toBeLessThan(points * 8);
  });

  it("hostParts.of gives one building's own roof, as hostHeightAt of that building alone", () => {
    const low = hipped(0, 0, 3, 5);
    const high = hipped(7, 2, 3.5, 7);
    const r = result([low, { faces: [], lines: [] }, high]);
    const h = hostParts(r);
    for (let x = -1; x <= 18.5; x += 0.5) {
      for (let y = -1; y <= 11; y += 0.5) {
        const p = { x, y };
        r.parts.forEach((part, i) => expect(h.of(i, p)).toBe(hostHeightAt({ ...r, parts: [part] }, p)));
        expect(h.top(p)).toBe(hostHeightAt(r, p));
      }
    }
    expect(h.of(7, { x: 1, y: 1 })).toBeNull();
  });
});

// UI-602: the 3D view tested every wall against every roof line of every
// building at each rebuild (quadratic in the buildings).
describe("crossingIndex (3D walls against roof lines)", () => {
  const allLines = (r: CalcResult) => r.parts.flatMap((rp) => rp.geometry.lines.map((l): [Poly3[number], Poly3[number]] => [l.a, l.b]));

  it("gives the same crossings as testing every line, for walls along, across and between buildings", () => {
    const r = result([hipped(0, 0, 3, 5), hipped(7, 2, 3.5, 7), hipped(30, 0)]);
    const lines = allLines(r);
    const at = crossingIndex(r.parts);
    const walls: [number, number, number, number][] = [];
    for (let i = 0; i < 400; i++) {
      // A fixed pattern of wall ends over and around the three buildings.
      const f = (k: number, span: number, off: number) => ((i * k) % 97) * (span / 97) + off;
      walls.push([f(7, 44, -2), f(11, 14, -2), f(13, 44, -2), f(17, 14, -2)]);
    }
    // Walls of the buildings themselves and ones ending exactly on a line.
    walls.push([0, 0, 10, 0], [10, 0, 10, 8], [7, 2, 17, 2], [4, 4, 6, 4], [0, 4, 40, 4], [5, 0, 5, 4]);
    let some = 0;
    for (const [ax, ay, bx, by] of walls) {
      const [a, b] = [{ x: ax, y: ay }, { x: bx, y: by }];
      const want = crossings(a, b, lines);
      expect(at(a, b)).toEqual(want);
      some += want.length;
    }
    expect(some).toBeGreaterThan(100);
    expect(crossingIndex(null)({ x: 0, y: 0 }, { x: 1, y: 1 })).toEqual([]);
  });

  it("tests each wall of the estate only against its own building's lines", () => {
    const r = estate();
    const stats = { lines: 0 };
    const at = crossingIndex(r.parts, stats);
    let walls = 0;
    for (let i = 0; i < 200; i++) {
      const [ox, oy] = [(i % 20) * 14, Math.floor(i / 20) * 12];
      const c = [
        { x: ox, y: oy },
        { x: ox + 10, y: oy },
        { x: ox + 10, y: oy + 8 },
        { x: ox, y: oy + 8 },
      ];
      c.forEach((a, k) => {
        const before = stats.lines;
        at(a, c[(k + 1) % 4]);
        // Nine lines per building; a wall on the building's edge meets at most those.
        expect(stats.lines - before).toBeLessThanOrEqual(9);
        walls++;
      });
    }
    expect(walls).toBe(800);
    // Testing every line for every wall would be 800 x 1800.
    expect(stats.lines).toBeLessThanOrEqual(800 * 9);
  });
});
