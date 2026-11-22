import { beforeEach, describe, expect, it } from "vitest";
import { changedEdges, changedItems, describeChange, describeFigure, figureName, ghostOf } from "./changes";
import { i18n } from "./i18n.svelte";
import type { CalcResult, Change, EdgeDef, Project } from "./types";
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

  it("shows the previous roof lines of a pitch change, not the ones that stayed (UX-017)", () => {
    const before = { ...emptyProject("x"), parts: [rect(10)] };
    const now = { ...emptyProject("x"), parts: [rect(10)] };
    const roof = (ridgeY: number) =>
      ({
        parts: [
          {
            geometry: {
              lines: [
                { kind: "eave", a: [0, 0, 3], b: [10, 0, 3] },
                { kind: "ridge", a: [2, ridgeY, 6], b: [8, ridgeY, 6] },
              ],
            },
          },
        ],
      }) as unknown as CalcResult;
    const changes: Change[] = [{ kind: "modified", field: "pitch_deg", part: 0, edge: 0, before: 40, after: 35 }];
    const g = ghostOf(changes, before, now, roof(4.5), roof(4));
    expect(g.lines).toEqual([[{ x: 2, y: 4 }, { x: 8, y: 4 }]]);
    // A change that does not reshape the roof draws no old lines.
    const named: Change[] = [{ kind: "modified", field: "name", before: "a", after: "b" }];
    expect(ghostOf(named, before, now, roof(4.5), roof(4)).lines).toEqual([]);
    // Without the previous roof only outlines are shown, as before.
    expect(ghostOf(changes, before, now, roof(4.5), null).lines).toEqual([]);
  });
});

describe("figures", () => {
  it("gives before, after and the signed difference", () => {
    const p = emptyProject("x");
    expect(describeFigure({ key: "roof_area", stored: 182.4, current: 195.12 }, p)).toBe("Roof area 182.40 → 195.12 m² (+12.72)");
    expect(describeFigure({ key: "line:F", stored: 10, current: 8.5 }, p)).toBe("F 10.00 → 8.50 m (−1.50)");
  });
});

describe("round-3 audit", () => {
  it("names an AI status change with the status before and after (UX-207)", () => {
    const p = emptyProject("x");
    expect(describeChange({ kind: "modified", field: "job.status", before: "draft", after: "ordered" }, p)).toBe("Status: Draft → Ordered");
  });
  it("names a line in the stored-offer comparison with its code and name (UX-205)", () => {
    const p = emptyProject("x");
    expect(figureName("line:OL", p)).toBe("OL Verge left");
    expect(figureName("roof_area", p)).toBe("Roof area");
  });
});

describe("malformed changes from a received change log (HUNT-301)", () => {
  const p = () => ({ ...emptyProject("x"), parts: [rect(12)] });
  const bad: unknown[] = [
    { kind: "modified", field: "vertex", part: 0, edge: 1, before: null, after: null },
    { kind: "modified", field: "vertex", part: 0, edge: 1, before: null, after: "x" },
    { kind: "modified", field: "windows", before: null, after: null },
    { kind: "modified", field: "dormers", before: null, after: 7 },
    { kind: "modified", field: "half_hip", part: 0, edge: 0, before: null, after: 3 },
    { kind: "added", field: "chimney", item: "x", before: null, after: null },
    { kind: "modified", field: 7, before: null, after: null },
    { kind: "removed", field: "chimney", item: 0, before: { corners: null }, after: null },
    { kind: "removed", field: "manual_line", item: 0, before: { a: null }, after: null },
    { kind: "removed", field: "dormer", item: 0, before: { edges: null }, after: null },
    { kind: "removed", field: "manual_face", item: 0, before: "x", after: null },
    null,
    "text",
  ];

  it("describes every one without throwing", () => {
    for (const c of bad) {
      expect(() => describeChange(c as Change, p())).not.toThrow();
      expect(typeof describeChange(c as Change, p())).toBe("string");
    }
  });

  it("highlights and ghosts them without throwing", () => {
    expect(() => changedEdges(bad as Change[], p())).not.toThrow();
    expect(() => changedItems(bad as Change[])).not.toThrow();
    const g = ghostOf(bad as Change[], p(), p(), null);
    for (const s of g.shapes) expect(Array.isArray(s)).toBe(true);
    for (const l of g.lines) expect(l.every((pt) => typeof pt?.x === "number")).toBe(true);
  });

  it("shows texts without control or bidi characters and cut to a line", () => {
    const name = describeChange({ kind: "modified", field: "name", before: "a", after: "Faktura ‮fdp.exe\u0007" }, p());
    expect(name).toBe("Name: Faktura fdp.exe");
    const long = describeChange({ kind: "modified", field: "job.customer", before: "", after: "x".repeat(100_000) }, p());
    expect(long.length).toBeLessThan(300);
  });

  // Audit 2026-11-22 TEST-405 (C2, C3, C7): what the guards stand for, not only "no throw".
  it("shows an unknown status or job field as another change", () => {
    const other = describeChange(null as unknown as Change, null);
    expect(describeChange({ kind: "modified", field: "job.status", before: "draft", after: "evil" } as Change, p())).toBe(other);
    expect(describeChange({ kind: "modified", field: "job.status", before: "evil", after: "offer" } as Change, p())).toBe(other);
    expect(describeChange({ kind: "modified", field: "job.foo", before: "", after: "x" } as Change, p())).toBe(other);
    expect(describeChange({ kind: "modified", field: "job.customer", before: "", after: "Novák" } as Change, p())).not.toBe(other);
  });

  it("puts no building prefix on a part index that is not a whole number", () => {
    const two = { ...emptyProject("x"), parts: [rect(12), rect(8)] };
    const named = (part: unknown) => describeChange({ kind: "modified", field: "part_name", part, before: "", after: "A" } as Change, two);
    expect(named(1)).toMatch(/2/);
    for (const part of ["x", 1.5, -1, null]) {
      const shown = named(part);
      expect(shown).not.toMatch(/x1|NaN|undefined|1\.5|-1/);
      expect(shown).toBe(describeChange({ kind: "modified", field: "part_name", before: "", after: "A" } as Change, two));
    }
  });
});
