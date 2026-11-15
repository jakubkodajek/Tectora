import { describe, expect, it } from "vitest";
import { anchor, stillThere } from "./anchor";

const house = { name: "Main", edges: [{ x: 0, y: 0 }] };
const garage = { name: "Garage", edges: [{ x: 28, y: 0 }] };

describe("dialog anchor (ERRU-010)", () => {
  it("holds while the part is unchanged", () => {
    const a = anchor([house, garage], 0);
    expect(stillThere([structuredClone(house), garage], a)).toBe(true);
  });

  it("notices that the part was removed and another one took its index", () => {
    const a = anchor([house, garage], 0);
    expect(stillThere([garage], a)).toBe(false);
  });

  it("notices that the part was removed from the end", () => {
    const a = anchor([house, garage], 1);
    expect(stillThere([house], a)).toBe(false);
    expect(stillThere(undefined, a)).toBe(false);
  });

  it("notices that the part was changed", () => {
    const a = anchor([house, garage], 1);
    expect(stillThere([house, { ...garage, edges: [{ x: 31, y: 0 }] }], a)).toBe(false);
  });
});
