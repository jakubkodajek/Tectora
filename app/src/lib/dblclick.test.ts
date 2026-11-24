import { describe, expect, it } from "vitest";
import { FRESH_BUTTON_MS, SAME_SPOT_PX, firstPress, pressTooSoon, secondPressOnFresh } from "./dblclick";

describe("firstPress (UI-008)", () => {
  it("opens the row of the first press when the rows moved under the second", () => {
    const p = firstPress<string>();
    p.press("novak", 1, 1000);
    // The second press landed on the row that moved under the pointer.
    p.press("dvorak", 2, 1200);
    expect(p.target("dvorak", 1210)).toBe("novak");
  });

  it("uses the reported row without a recent first press", () => {
    const p = firstPress<string>();
    expect(p.target("dvorak", 5000)).toBe("dvorak");
    p.press("novak", 1, 1000);
    expect(p.target("dvorak", 3000)).toBe("dvorak");
  });

  it("forgets the first press once a double click used it", () => {
    const p = firstPress<string>();
    p.press("novak", 1, 1000);
    expect(p.target("novak", 1200)).toBe("novak");
    expect(p.target("dvorak", 1300)).toBe("dvorak");
  });
});

describe("pressTooSoon (UI-103)", () => {
  it("ignores the second click of a double click that lands on a button that just appeared", () => {
    const shown = 1000;
    expect(pressTooSoon(shown, 1, 1150)).toBe(true);
    expect(pressTooSoon(shown, 2, 1300)).toBe(true);
  });

  it("takes a single press once the button has been there a moment", () => {
    expect(pressTooSoon(1000, 1, 1000 + FRESH_BUTTON_MS + 1)).toBe(false);
  });

  it("ignores a double click's second click whenever it comes", () => {
    expect(pressTooSoon(1000, 2, 5000)).toBe(true);
  });
});

describe("secondPressOnFresh (UI-208)", () => {
  const origin = { x: 500, y: 700, at: 1000 };
  it("ignores the double click's second press where the swapped bar now has another button", () => {
    expect(secondPressOnFresh(origin, 1010, { x: 501, y: 700, at: 1090 })).toBe(true);
  });
  it("lets a press elsewhere on the new bar or a later one through", () => {
    expect(secondPressOnFresh(origin, 1010, { x: 640, y: 700, at: 1090 })).toBe(false);
    expect(secondPressOnFresh(origin, 1010, { x: 500, y: 700, at: 1010 + FRESH_BUTTON_MS })).toBe(false);
  });
  it("does not count a press long before the bar appeared as its cause", () => {
    expect(secondPressOnFresh(origin, 1000 + FRESH_BUTTON_MS + 1, { x: 500, y: 700, at: 1600 })).toBe(false);
    expect(secondPressOnFresh(null, 1010, { x: 500, y: 700, at: 1090 })).toBe(false);
  });
  // Audit 2026-11-22 TEST-404 (V9): the same spot is at most 8 px away.
  it("counts a press 8 px away as the same spot and 9 px away as another", () => {
    expect(SAME_SPOT_PX).toBe(8);
    expect(secondPressOnFresh(origin, 1010, { x: 508, y: 700, at: 1090 })).toBe(true);
    expect(secondPressOnFresh(origin, 1010, { x: 500, y: 709, at: 1090 })).toBe(false);
  });
});
