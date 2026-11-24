import { describe, expect, it } from "vitest";
import { sequence } from "./latest";

describe("sequence (audit 2026-11-24 ERR-1902, ERR-1904, ERR-1905, ERR-1906)", () => {
  it("applies only the newest request's answer, whatever order they land in", async () => {
    const seq = sequence();
    let shown = "";
    const read = (value: string, ms: number) => {
      const k = seq.next();
      return new Promise<void>((done) =>
        setTimeout(() => {
          if (seq.current(k)) shown = value;
          done();
        }, ms),
      );
    };
    // The first read is slow and lands after the second.
    await Promise.all([read("old", 30), read("new", 5)]);
    expect(shown).toBe("new");
  });

  it("drops every answer on its way after a bump", () => {
    const seq = sequence();
    const k = seq.next();
    expect(seq.current(k)).toBe(true);
    seq.bump();
    expect(seq.current(k)).toBe(false);
    expect(seq.current(seq.next())).toBe(true);
  });
});
