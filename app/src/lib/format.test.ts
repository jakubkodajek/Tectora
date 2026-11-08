import { describe, expect, it } from "vitest";
import { plain } from "./format";

describe("plain (intermediate values in the calculation breakdown, UX-014)", () => {
  it("drops single-precision noise", () => {
    expect(plain(58.499997, "de-DE")).toBe("58,50");
    expect(plain(3.875, "cs-CZ")).toBe("3,875");
    expect(plain(3.875000001, "en-GB")).toBe("3.875");
  });

  it("keeps real decimals up to four and at least two", () => {
    expect(plain(12.34567, "en-GB")).toBe("12.3457");
    expect(plain(12, "cs-CZ")).toBe("12,00");
    expect(plain(-0, "cs-CZ")).toBe("0,00");
  });
});
