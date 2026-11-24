import { describe, expect, it } from "vitest";
import { bannerChanges, beforeKey } from "./reviewbanner";
import type { ActivityItem, Change, Project, ReviewStatus } from "./types";

const change = (after: number): Change => ({ kind: "modified", field: "overhang", part: 0, edge: 0, before: 0.5, after }) as Change;
const review = (over: Partial<ReviewStatus> = {}): ReviewStatus => ({
  since_ts_ms: 10,
  count: 1,
  actors: ["claude"],
  steps: [10],
  before: {} as Project,
  accepted_ms: 0,
  differences: null,
  ...over,
});

describe("the review banner (UX-2001, ARCH-2004)", () => {
  it("calculates the roof before again when the user's own edit moved the baseline", () => {
    const one = beforeKey(review({ before_rev: "a" }));
    const two = beforeKey(review({ before_rev: "b" }));
    expect(one).not.toBe(two);
    expect(beforeKey(review({ before_rev: "a" }))).toBe(one);
    expect(beforeKey(review({ before: null }))).toBe("");
    expect(beforeKey(null)).toBe("");
  });

  it("shows the net difference against the approved state, not each step's own", () => {
    const steps = [{ changes: [change(0.9)] }, { changes: [change(0.6)] }] as ActivityItem[];
    const net = [change(0.7)];
    expect(bannerChanges(review({ changes: net }), steps)).toEqual(net);
    expect(bannerChanges(review({ changes: [] }), steps)).toEqual([]);
    // An older bridge: the listed steps' changes.
    expect(bannerChanges(review(), steps)).toEqual([change(0.9), change(0.6)]);
  });
});
