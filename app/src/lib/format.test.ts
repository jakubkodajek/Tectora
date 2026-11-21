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

describe("library card of a roof the core refused (ERRU-005)", () => {
  it("shows a dash, not 0,00 m², when no building has a roof", async () => {
    const { cardArea, cardAreaNote, cardRoof } = await import("./format");
    const p = { roof_area: 0, parts: 1, parts_failed: 1 };
    expect(cardRoof(p)).toBe("none");
    expect(cardArea(p)).toBe("—");
    expect(cardAreaNote(p)).toBeTruthy();
  });

  it("marks a job where some buildings have no roof as partly calculated", async () => {
    const { cardArea, cardAreaNote, cardRoof, m2 } = await import("./format");
    const p = { roof_area: 60.67, parts: 2, parts_failed: 1 };
    expect(cardRoof(p)).toBe("partial");
    expect(cardArea(p)).toBe(m2(60.67));
    expect(cardAreaNote(p)).toBeTruthy();
  });

  it("leaves a complete roof unmarked", async () => {
    const { cardAreaNote, cardRoof } = await import("./format");
    expect(cardRoof({ roof_area: 12, parts: 1, parts_failed: 0 })).toBe("ok");
    expect(cardAreaNote({ roof_area: 12, parts: 1, parts_failed: 0 })).toBeUndefined();
  });
});

describe("attachment sizes follow the UI language (UI-010)", () => {
  it("uses the locale's decimal separator", async () => {
    const { fileSize } = await import("./format");
    expect(fileSize(1.5 * 1048576, "cs-CZ")).toBe("1,5 MB");
    expect(fileSize(1.5 * 1048576, "de-DE")).toBe("1,5 MB");
    expect(fileSize(1.5 * 1048576, "en-GB")).toBe("1.5 MB");
    expect(fileSize(152 * 1024, "sk-SK")).toBe("152 kB");
    expect(fileSize(512, "cs-CZ")).toBe("512 B");
    expect(fileSize(NaN, "cs-CZ")).toBe("—");
  });
});
