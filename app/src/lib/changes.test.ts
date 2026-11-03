import { beforeEach, describe, expect, it } from "vitest";
import { changedItems, describeChange, describeFigure, ghostOf } from "./changes";
import { i18n } from "./i18n.svelte";
import type { Change, EdgeDef, Project } from "./types";
import { emptyProject } from "./types";

const edge = (x: number, y: number): EdgeDef => ({ start: { x, y }, pitch_deg: 40, eave_height: 3, overhang: 0.5 }) as EdgeDef;
const rect = (w: number): Project["parts"][number] => ({
  name: "Dům",
  edges: [edge(0, 0), edge(w, 0), edge(w, 8), edge(0, 8)],
});
const chimney = (x: number) => ({ corners: [{ x, y: 2 }, { x: x + 0.5, y: 2 }, { x: x + 0.5, y: 2.5 }, { x, y: 2.5 }], height: 9 });

beforeEach(() => {
  i18n.locale = "en";
});

describe("item changes", () => {
  it("names the item and what happened to it", () => {
    const p = emptyProject("x");
    const c = (kind: Change["kind"], field: string, item: number): Change => ({ kind, field, item, before: null, after: null });
    expect(describeChange(c("added", "chimney", 1), p)).toBe("Added: Chimney 2");
    expect(describeChange(c("removed", "window", 0), p)).toBe("Removed: Window 1");
    expect(describeChange(c("modified", "dormer", 2), p)).toBe("Changed: Dormer 3");
    expect(describeChange(c("modified", "manual_line", 0), p)).toBe("Changed: Line 1");
  });

  it("still reads logs written before item changes", () => {
    const p = emptyProject("x");
    expect(describeChange({ kind: "modified", field: "dormers", before: [], after: [{}, {}] }, p)).toBe("Dormers: 2");
    expect(describeChange({ kind: "modified", field: "roof_openings", before: [], after: [{}] }, p)).toBe("Openings: 1");
  });

  it("highlights added and changed items, not removed ones", () => {
    const keys = changedItems([
      { kind: "added", field: "chimney", item: 1, before: null, after: {} },
      { kind: "modified", field: "dormer", item: 0, before: {}, after: {} },
      { kind: "removed", field: "window", item: 0, before: {}, after: null },
      { kind: "modified", field: "pitch_deg", part: 0, edge: 1, before: 40, after: 45 },
    ]);
    expect([...keys].sort()).toEqual(["chimney:1", "dormer:0"]);
  });
});

describe("previous state", () => {
  it("shows the old outline of a moved wall and the old place of a moved chimney", () => {
    const before = { ...emptyProject("x"), parts: [rect(10)], chimneys: [chimney(2)] };
    const now = { ...emptyProject("x"), parts: [rect(12)], chimneys: [chimney(4)] };
    const changes: Change[] = [
      { kind: "modified", field: "vertex", part: 0, edge: 1, before: [10, 0], after: [12, 0] },
      { kind: "modified", field: "chimney", item: 0, before: chimney(2), after: chimney(4) },
    ];
    const g = ghostOf(changes, before, now, null);
    expect(g.shapes).toHaveLength(2);
    expect(g.shapes[0][1]).toEqual({ x: 10, y: 0 });
    expect(g.shapes[1][0]).toEqual({ x: 2, y: 2 });
  });

  it("draws no walls when only a pitch changed", () => {
    const before = { ...emptyProject("x"), parts: [rect(10)] };
    const changes: Change[] = [{ kind: "modified", field: "pitch_deg", part: 0, edge: 0, before: 40, after: 45 }];
    expect(ghostOf(changes, before, before, null).shapes).toEqual([]);
    expect(ghostOf(changes, null, before, null)).toEqual({ shapes: [], lines: [] });
  });
});

describe("figures", () => {
  it("gives before, after and the signed difference", () => {
    const p = emptyProject("x");
    expect(describeFigure({ key: "roof_area", stored: 182.4, current: 195.12 }, p)).toBe("Roof area 182.40 → 195.12 m² (+12.72)");
    expect(describeFigure({ key: "line:F", stored: 10, current: 8.5 }, p)).toBe("F 10.00 → 8.50 m (−1.50)");
  });
});
