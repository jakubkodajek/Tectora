import { describe, expect, it } from "vitest";
import { keepMissedNotice, takeMissedNotice } from "./missed";

const entry = (ts: number) => ({ entry_ts_ms: ts, summary: `Stored result recalculated ${ts}` });

describe("a recalculation notice a left editor could not show (ERR-902)", () => {
  it("is given once to the next editor of the same job, and to no other job", () => {
    keepMissedNotice("a", entry(1));
    expect(takeMissedNotice("b")).toBeNull();
    expect(takeMissedNotice("a")).toEqual(entry(1));
    expect(takeMissedNotice("a")).toBeNull();
  });
  it("keeps the newest notice per job", () => {
    keepMissedNotice("a", entry(1));
    keepMissedNotice("a", entry(2));
    expect(takeMissedNotice("a")).toEqual(entry(2));
  });
});
