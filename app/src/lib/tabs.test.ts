import { describe, expect, it } from "vitest";
import { rovingTarget } from "./tabs";

describe("rovingTarget (UI-007)", () => {
  it("moves with the arrows and wraps around", () => {
    expect(rovingTarget("ArrowRight", 0, 3)).toBe(1);
    expect(rovingTarget("ArrowDown", 2, 3)).toBe(0);
    expect(rovingTarget("ArrowLeft", 0, 3)).toBe(2);
    expect(rovingTarget("ArrowUp", 1, 3)).toBe(0);
  });
  it("jumps with Home / End and ignores other keys", () => {
    expect(rovingTarget("Home", 2, 4)).toBe(0);
    expect(rovingTarget("End", 0, 4)).toBe(3);
    expect(rovingTarget("Enter", 0, 4)).toBeNull();
    expect(rovingTarget("ArrowRight", -1, 4)).toBeNull();
    expect(rovingTarget("ArrowRight", 0, 0)).toBeNull();
  });
});
