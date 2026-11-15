import { describe, expect, it } from "vitest";
import { stepWall, wallStepKey } from "./wallstep";

const wall = { start: { x: 0, y: 0 }, pitch_deg: 30, eave_height: 3, overhang: 0.5 };
const project = { parts: [{ name: "A", edges: [wall, wall, wall, wall] }, { name: "B", edges: [wall, wall, wall] }] } as never;

describe("stepWall (UX-006)", () => {
  it("steps through the walls of all buildings and wraps around", () => {
    expect(stepWall(project, { part: 0, edge: 0 }, 1)).toEqual({ part: 0, edge: 1 });
    expect(stepWall(project, { part: 0, edge: 3 }, 1)).toEqual({ part: 1, edge: 0 });
    expect(stepWall(project, { part: 1, edge: 2 }, 1)).toEqual({ part: 0, edge: 0 });
    expect(stepWall(project, { part: 0, edge: 0 }, -1)).toEqual({ part: 1, edge: 2 });
  });
  it("starts at the first wall (next) or the last (previous) with nothing selected", () => {
    expect(stepWall(project, null, 1)).toEqual({ part: 0, edge: 0 });
    expect(stepWall(project, null, -1)).toEqual({ part: 1, edge: 2 });
    expect(stepWall({ parts: [] } as never, null, 1)).toBeNull();
  });
});

describe("wallStepKey", () => {
  const k = (key: string, mods: Partial<KeyboardEvent> = {}) => ({ key, altKey: false, ctrlKey: false, metaKey: false, shiftKey: false, ...mods });
  it("PageDown / PageUp step everywhere", () => {
    expect(wallStepKey(k("PageDown"), false)).toBe(1);
    expect(wallStepKey(k("PageUp"), true)).toBe(-1);
  });
  it("Alt+arrows step outside text fields, Shift+Enter inside them", () => {
    expect(wallStepKey(k("ArrowRight", { altKey: true }), false)).toBe(1);
    expect(wallStepKey(k("ArrowLeft", { altKey: true }), false)).toBe(-1);
    expect(wallStepKey(k("ArrowRight", { altKey: true }), true)).toBe(0);
    expect(wallStepKey(k("Enter", { shiftKey: true }), true)).toBe(1);
    expect(wallStepKey(k("Enter", { shiftKey: true }), false)).toBe(0);
    expect(wallStepKey(k("Enter"), true)).toBe(0);
    expect(wallStepKey(k("ArrowRight"), false)).toBe(0);
    expect(wallStepKey(k("PageDown", { ctrlKey: true }), false)).toBe(0);
  });
});
