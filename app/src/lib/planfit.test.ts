import { describe, expect, it } from "vitest";
import { LABEL_PAD, boxInView, boxOf, draftLeavesView, fitView, resizedView } from "./planfit";

describe("fitView keeps room for the wall labels (UI-010)", () => {
  it("leaves at least LABEL_PAD px on every side of the eaves", () => {
    // A 17 m wide roof in a 367 px plan (1024 px window, split layout).
    const box = { minX: -0.5, minY: -0.5, maxX: 16.5, maxY: 14.5 };
    const v = fitView(box, 367, 580)!;
    expect(boxInView(box, v, 367, 580, LABEL_PAD - 0.01)).toBe(true);
  });

  it("keeps the quarter margin in a large view", () => {
    const v = fitView({ minX: 0, minY: 0, maxX: 10, maxY: 5 }, 800, 600)!;
    expect(v.scale).toBe(60);
    expect(v.center).toEqual({ x: 5, y: 2.5 });
  });

  it("still fits a tiny view and refuses one with no room", () => {
    expect(fitView({ minX: 0, minY: 0, maxX: 10, maxY: 5 }, 100, 60)!.scale).toBeGreaterThan(0);
    expect(fitView({ minX: 0, minY: 0, maxX: 10, maxY: 5 }, 800, 0)).toBeNull();
  });

  it("boxOf refuses coordinates that are not finite", () => {
    expect(boxOf([{ x: 0, y: NaN }])).toBeNull();
    expect(boxOf([])).toBeNull();
    expect(boxOf([{ x: 1, y: 2 }, { x: -1, y: 5 }])).toEqual({ minX: -1, minY: 2, maxX: 1, maxY: 5 });
  });
});

describe("resizedView: the plan does not jump when the bar below it grows (UI-011)", () => {
  const box = { minX: 0, minY: 0, maxX: 12, maxY: 8 };
  const sy = (v: { scale: number; center: { y: number } }, h: number, y: number) => -(y - v.center.y) * v.scale + h / 2;

  it("keeps every wall at the same screen position when only the height changes", () => {
    const v = fitView(box, 900, 658)!;
    const next = resizedView(v, { width: 900, height: 658 }, { width: 900, height: 579 }, box);
    expect(next).not.toBe("fit");
    const n = next as Exclude<typeof next, "fit">;
    expect(n.scale).toBe(v.scale);
    for (const y of [0, 4, 8]) expect(sy(n, 579, y)).toBeCloseTo(sy(v, 658, y), 6);
  });

  it("fits again when the width changes or the drawing would leave the view", () => {
    const v = fitView(box, 900, 658)!;
    expect(resizedView(v, { width: 900, height: 658 }, { width: 700, height: 658 }, box)).toBe("fit");
    expect(resizedView(v, { width: 900, height: 658 }, { width: 900, height: 300 }, box)).toBe("fit");
    expect(resizedView(v, { width: 900, height: 658 }, { width: 900, height: 600 }, null)).toBe("fit");
  });
});

describe("draftLeavesView: the plan follows a footprint being typed (UX-001)", () => {
  const view = { scale: 30, center: { x: 8, y: 5 } };
  it("is false while the newest corner is in view", () => {
    expect(draftLeavesView([{ x: 2, y: 2 }, { x: 10, y: 2 }], view, 800, 600)).toBe(false);
    expect(draftLeavesView([], view, 800, 600)).toBe(false);
  });
  it("is true when a typed wall puts the corner outside the plan", () => {
    expect(draftLeavesView([{ x: 2, y: 2 }, { x: 26, y: 2 }], view, 800, 600)).toBe(true);
    expect(draftLeavesView([{ x: 2, y: 2 }, { x: 2, y: 16 }], view, 800, 600)).toBe(true);
  });
});
