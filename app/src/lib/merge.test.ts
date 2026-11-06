import { describe, expect, it } from "vitest";
import { merge3 } from "./merge";

const wall = (pitch: number, overhang = 0.5) => ({ start: { x: 0, y: 0 }, pitch_deg: pitch, eave_height: 3, overhang });

describe("merge3", () => {
  it("keeps a local edit and an external edit of different walls", () => {
    const base = { name: "A", parts: [{ edges: [wall(35), wall(35)] }] };
    const local = { name: "A", parts: [{ edges: [wall(35, 0.9), wall(35)] }] };
    const remote = { name: "A", parts: [{ edges: [wall(35), wall(44)] }] };
    expect(merge3(base, local, remote)).toEqual({ name: "A", parts: [{ edges: [wall(35, 0.9), wall(44)] }] });
  });

  it("keeps different fields of the same wall", () => {
    const base = { e: wall(35) };
    expect(merge3(base, { e: wall(35, 0.9) }, { e: wall(40) })).toEqual({ e: wall(40, 0.9) });
  });

  it("lets the external edit win where both changed the same value", () => {
    expect(merge3({ p: 35 }, { p: 30 }, { p: 44 })).toEqual({ p: 44 });
  });

  it("takes the external list when the number of walls changed", () => {
    const base = { edges: [1, 2, 3, 4] };
    expect(merge3(base, { edges: [1, 9, 3, 4] }, { edges: [1, 2, 3] })).toEqual({ edges: [1, 2, 3] });
    expect(merge3(base, { edges: [1, 2, 3, 4, 5] }, base)).toEqual({ edges: [1, 2, 3, 4, 5] });
  });

  it("ignores nulls and empty lists the store leaves out", () => {
    const base = { a: 1 };
    const local = { a: 1, half_hip: null, breaks: [] };
    const remote = { a: 2 };
    expect(merge3(base, local, remote)).toEqual({ a: 2 });
  });

  it("adds keys added on either side and drops keys removed on one side", () => {
    expect(merge3({ a: 1, b: 2 }, { a: 1, b: 2, c: 3 }, { a: 1 })).toEqual({ a: 1, c: 3 });
  });
});
