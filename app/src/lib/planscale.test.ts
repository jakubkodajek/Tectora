// A job at the limits (200 buildings, check_limits) must not freeze the plan:
// the covered stretches of every roof line and the face labels were worked
// out by testing every sample against every face of every building through
// the reactive result (UI-501, TEST-501). Display only.

import { describe, expect, it } from "vitest";
import { hostHeightAt, hostIndex } from "./dormers";
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
