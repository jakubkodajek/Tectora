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

describe("library card figures still being calculated (ARCH-001)", () => {
  it("shows an ellipsis instead of 0,00 m² while the figures are pending", async () => {
    const { cardArea, m2 } = await import("./format");
    expect(cardArea({ roof_area: 0, figures_pending: true })).toBe("…");
    expect(cardArea({ roof_area: 12.5 })).toBe(m2(12.5));
  });

  it("asks again with growing pauses and gives up after eight tries", async () => {
    const { pendingRetryMs } = await import("./format");
    expect(pendingRetryMs(false, 0)).toBeNull();
    expect([0, 1, 2, 5].map((n) => pendingRetryMs(true, n))).toEqual([1000, 2000, 4000, 30000]);
    expect(pendingRetryMs(true, 8)).toBeNull();
  });
});
