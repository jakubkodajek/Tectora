import { describe, expect, it } from "vitest";
import { kindOf, MANUAL_CODES } from "./manual";

describe("plan colour of a hand-drawn line (TEST-1403)", () => {
  it("gives the counted codes their kind and every other code, prototype names too, none", () => {
    expect(kindOf("T")).toBe("eave");
    expect(kindOf("WS")).toBe("abutment");
    expect(kindOf("M")).toBeNull();
    expect(MANUAL_CODES.filter((c) => kindOf(c)).length).toBe(13);
    for (const code of ["constructor", "toString", "__proto__", "hasOwnProperty"]) expect(kindOf(code), code).toBeNull();
  });
});
