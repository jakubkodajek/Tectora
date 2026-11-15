import { describe, expect, it } from "vitest";
import { firstPress } from "./dblclick";

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
