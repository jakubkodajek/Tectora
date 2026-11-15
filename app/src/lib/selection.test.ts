import { describe, expect, it } from "vitest";
import { keptSelection } from "./selection";
import { emptyProject, type Project } from "./types";

function job(walls: number[]): Project {
  const p = emptyProject("Test");
  p.parts = walls.map((n, i) => ({
    name: `P${i}`,
    edges: Array.from({ length: n }, (_, k) => ({ start: { x: k, y: k % 2 }, pitch_deg: 30, eave_height: 3, overhang: 0.5 })),
  })) as Project["parts"];
  return p;
}

describe("keptSelection (undo / redo, UX-005)", () => {
  it("keeps the selected wall when it still exists", () => {
    expect(keptSelection(job([4]), { part: 0, edge: 1 })).toEqual({ part: 0, edge: 1 });
    expect(keptSelection(job([4, 5]), { part: 1, edge: 4 })).toEqual({ part: 1, edge: 4 });
  });

  it("drops it when the wall or the part is gone", () => {
    expect(keptSelection(job([3]), { part: 0, edge: 3 })).toBeNull();
    expect(keptSelection(job([4]), { part: 1, edge: 0 })).toBeNull();
    expect(keptSelection(job([]), { part: 0, edge: 0 })).toBeNull();
    expect(keptSelection(null, { part: 0, edge: 0 })).toBeNull();
    expect(keptSelection(job([4]), null)).toBeNull();
  });
});
