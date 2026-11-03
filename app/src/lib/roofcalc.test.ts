// The calculation helpers against values DACAD DE.exe itself stored
// (2026-10-29; dialogs driven under Wine, results read back from the saved
// project). Projects: 10 × 8, walls counter-clockwise from (0, 0), eave 3.0,
// overhang 0.5, pitch 40° (gable ends 90° where named).
import { describe, expect, it } from "vitest";
import {
  buildUp,
  hipPitches,
  hipPitchesFromEaves,
  meeting,
  mergeOnto,
  parallelEdit,
  parallelPatches,
  parallelStart,
  eaveBelowGable,
  solve,
  shiftFor,
  triangle,
  type ParallelField,
  type ParallelTarget,
} from "./roofcalc";
import type { EdgeDef } from "./types";

const wall = (x: number, y: number, pitch = 40, extra: Partial<EdgeDef> = {}): EdgeDef => ({
  start: { x, y },
  pitch_deg: pitch,
  eave_height: 3,
  overhang: 0.5,
  ...extra,
});
const gable = () => [wall(0, 0), wall(10, 0, 90), wall(10, 8), wall(0, 8, 90)];
const hip = () => [wall(0, 0), wall(10, 0), wall(10, 8), wall(0, 8)];
const close = (got: number | undefined, want: number, d = 4) => expect(got).toBeCloseTo(want, d);

describe("Einfaches Dreieck", () => {
  const e = gable()[0];
  it("width without overhang and ridge height (Ta)", () => close(triangle(e, { width: 4, ridge: 6 }, false)?.pitch_deg, 33.69007));
  it("rafter and ridge height (Tc)", () => close(triangle(e, { rafter: 5.5, ridge: 6 }, false)?.pitch_deg, 33.05573));
  it("width and rafter (Td)", () => close(triangle(e, { width: 4, rafter: 5.5 }, false)?.pitch_deg, 35.0968));
  it("width with overhang", () => close(triangle(e, { width: 4.5, ridge: 6 }, true)?.pitch_deg, 33.69007));
  it("the last break as a plane of its own (Te)", () => {
    const k = wall(0, 0, 40, { breaks: [{ height: 4.5, pitch_deg: 60 }] });
    close(triangle(k, { width: 4, ridge: 7 }, false)?.pitch_deg, 29.0546);
  });
});

describe("2 Parallele", () => {
  // DACAD's "left (oben)" wall is the north one (direction 180°): wall 2.
  const run = (edges: EdgeDef[], target: ParallelTarget, steps: [ParallelField, number][], withOv = true) => {
    const a = edges[2];
    const b = edges[0];
    const at = meeting(a, b, 8)!;
    let st = parallelStart(a, b, 8, at);
    for (const [f, v] of steps) st = parallelEdit(st, f, v, target, withOv)!;
    return st;
  };
  const pp = (edges: EdgeDef[], target: "pitch" | "eave" | "overhang", steps: [ParallelField, number][], withOv = true) =>
    parallelPatches(run(edges, target, steps, withOv), target);
  it("pitch from a width, ridge kept (P2a)", () => {
    const [n, s] = pp(gable(), "pitch", [["widthA", 4]]);
    close(n.pitch_deg, 43.3496);
    close(s.pitch_deg, 37.0597);
  });
  it("pitch from the ridge height (P2b)", () => close(pp(gable(), "pitch", [["ridge", 7.5]])[0].pitch_deg, 45));
  it("pitch from both rafters (P2c)", () => {
    const [n, s] = pp(gable(), "pitch", [["rafterA", 6], ["rafterB", 5.5]]);
    close(n.pitch_deg, 36.5593);
    close(s.pitch_deg, 40.5269);
  });
  it("a flat strip keeps the pitches (P2d)", () => close(pp(gable(), "pitch", [["top", 1]])[0].pitch_deg, 40));
  it("width without overhang (P2e)", () => {
    const [n, s] = pp(gable(), "pitch", [["widthA", 3]], false);
    close(n.pitch_deg, 47.172);
    close(s.pitch_deg, 34.471);
  });
  it("eave heights from the ridge height (Ea)", () => close(pp(gable(), "eave", [["ridge", 7.5]])[1].eave_height, 3.72405));
  it("eave heights from a rafter, its own eave kept (Ec)", () => {
    const [n, s] = pp(gable(), "eave", [["rafterA", 6]]);
    close(n.eave_height, 3);
    close(s.eave_height, 3.16156);
  });
  it("eave heights from a width on a hip roof (Eg)", () => {
    const [n, s] = pp(hip(), "eave", [["widthA", 4]]);
    close(n.eave_height, 3.41955);
    close(s.eave_height, 2.58045);
  });
  it("DACAD refuses an eave below the gable's (Eb)", () => {
    const [, s] = pp(gable(), "eave", [["widthA", 4]]);
    expect(eaveBelowGable(gable(), 0, s.eave_height!)).toBe(true);
    expect(eaveBelowGable(hip(), 0, s.eave_height!)).toBe(false);
  });
  it("overhangs from a width: wall runs kept (Oa)", () => {
    const [n, s] = pp(gable(), "overhang", [["widthA", 4]]);
    close(n.overhang, 0);
    close(s.overhang, 1);
  });
  it("overhangs from the ridge height (Ob, Oe)", () => {
    close(pp(gable(), "overhang", [["ridge", 7.5]])[0].overhang, 1.36289);
    close(pp(gable(), "overhang", [["ridge", 6.5]])[1].overhang, 0.17114);
  });
  it("overhangs from a rafter (Oc)", () => {
    const [n, s] = pp(gable(), "overhang", [["rafterA", 6.5]]);
    close(n.overhang, 0.97929);
    close(s.overhang, 0.97929);
  });
  it("walls move for the ridge height (La)", () => {
    const st = run(gable(), "position", [["ridge", 7.5]]);
    close(st.shiftA, -0.8629);
    close(st.shiftB, -0.8629);
  });
  it("walls move for a width (Lb) and a rafter (Lc)", () => {
    close(run(gable(), "position", [["widthA", 4]]).shiftB, 0.5);
    close(run(gable(), "position", [["rafterA", 6]]).shiftA, -0.0963);
  });
});

describe("Punkt im Raum", () => {
  // Point (5, 4) at height 7 for the south wall: inward distance 4.
  const s = gable()[0];
  it("pitch (PRa)", () => close(solve(s, 4, 7, "pitch")?.pitch_deg, 41.63354));
  it("eave (PRe)", () => close(solve(s, 4, 7, "eave")?.eave_height, 3.22405));
  it("overhang (PRo)", () => close(solve(s, 4, 7, "overhang")?.overhang, 0.76701));
  it("pitch above a break, true break geometry (PRk)", () => {
    const k = wall(0, 0, 40, { breaks: [{ height: 4.5, pitch_deg: 60 }] });
    close(solve(k, 4, 7, "pitch")?.breaks?.[0].pitch_deg, 42.66687);
  });
  it("wall position (PRl)", () => close(shiftFor(s, 4, 7) ?? undefined, -0.267, 3));
});

describe("Flächen zusammenfassen", () => {
  // South wall stepped: (0,0)-(5,0), gable step to (5,1), (5,1)-(10,1).
  const step = () => [wall(0, 0), wall(5, 0, 90), wall(5, 1), wall(10, 1, 90), wall(10, 8), wall(0, 8, 90)];
  it("eave (FZe)", () => close(mergeOnto(step(), 0, [2], "eave").get(2)?.eave_height, 3.8391));
  it("overhang (FZo)", () => close(mergeOnto(step(), 0, [2], "overhang").get(2)?.overhang, 1.5));
});

describe("Grat/Kehle", () => {
  it("hip and both rafters (Ga)", () => {
    const m = hipPitches(hip(), 0, 1, 7.5, 5.5, 6)!;
    close(m.get(0)?.pitch_deg, 35.09679);
    close(m.get(1)?.pitch_deg, 31.8061);
  });
  it("hip and both eave pieces (Gb)", () => {
    const m = hipPitchesFromEaves(hip(), 0, 1, 7.5, 5, 4.5)!;
    close(m.get(0)?.pitch_deg, 36.39125, 3);
    close(m.get(1)?.pitch_deg, 33.55732);
  });
});

describe("Um Aufbaudicke korrigieren", () => {
  const s = gable()[0];
  it("normal (Aa)", () => {
    const p = buildUp(s, 0.2, "normal")!;
    close(p.eave_height, 3.15321);
    close(p.overhang, 0.62856);
  });
  it("horizontal (Ab)", () => close(buildUp(s, 0.2, "horizontal")!.overhang, 0.81114));
  it("vertical (Ac)", () => close(buildUp(s, 0.2, "vertical")!.eave_height, 3.26108));
});
