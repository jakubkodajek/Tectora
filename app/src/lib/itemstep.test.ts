import { describe, expect, it } from "vitest";
import { barTakesStep, itemStepKey, stepIndex } from "./itemstep";

const key = (k: string, mods: Partial<KeyboardEvent> = {}) => ({ key: k, altKey: false, ctrlKey: false, metaKey: false, shiftKey: false, ...mods });

describe("stepIndex: the ◀ ▶ of the dormer, opening and hand-drawn bars", () => {
  it("steps and wraps around", () => {
    expect(stepIndex(0, 3, 1)).toBe(1);
    expect(stepIndex(2, 3, 1)).toBe(0);
    expect(stepIndex(0, 3, -1)).toBe(2);
  });
  it("starts at the first (next) or the last (previous) with nothing selected", () => {
    expect(stepIndex(null, 3, 1)).toBe(0);
    expect(stepIndex(null, 3, -1)).toBe(2);
    expect(stepIndex(7, 3, 1)).toBe(0);
    expect(stepIndex(0, 0, 1)).toBeNull();
  });
});

describe("itemStepKey: PageDown / PageUp", () => {
  it("names the step of PageDown and PageUp only", () => {
    expect(itemStepKey(key("PageDown"))).toBe(1);
    expect(itemStepKey(key("PageUp"))).toBe(-1);
    expect(itemStepKey(key("ArrowRight"))).toBe(0);
    expect(itemStepKey(key("Enter", { shiftKey: true }))).toBe(0);
  });
  it("leaves modified keys to the browser and the system", () => {
    for (const m of ["ctrlKey", "metaKey", "altKey", "shiftKey"] as const) expect(itemStepKey(key("PageDown", { [m]: true }))).toBe(0);
  });
});

describe("barTakesStep", () => {
  it("steps from the bar and from the plan, not from another field", () => {
    expect(barTakesStep(true, true, false, false)).toBe(true);
    expect(barTakesStep(false, false, false, false)).toBe(true);
    expect(barTakesStep(false, true, false, false)).toBe(false);
  });
  it("a bar behind a dialog does not step; the bar in the dialog does", () => {
    expect(barTakesStep(false, false, true, false)).toBe(false);
    expect(barTakesStep(true, true, true, false)).toBe(false);
    expect(barTakesStep(true, true, true, true)).toBe(true);
    expect(barTakesStep(false, false, true, true)).toBe(false);
  });
});
