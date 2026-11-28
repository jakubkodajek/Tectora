// Round 2 of the towers audit, second pass: an arc straightened, a curve's
// largest value, a tier's pitch and the towers' limits on a copy.
import { describe, expect, it } from "vitest";
import { i18n } from "./i18n.svelte";
import { arcReason, describeError, refusalReason } from "./errors";

describe("an arc straightened or changed (UX-007)", () => {
  it("a wall of no arc is said in the user's language", () => {
    i18n.locale = "en";
    expect(arcReason("wall 2 is no part of an arc (arc_none)")).toBe("The wall is no part of an arc (at least 3 walls of one length whose corners turn alike).");
    i18n.locale = "cs";
    expect(refusalReason("wall 2 is no part of an arc (arc_none)")).toContain("není součástí oblouku");
  });
});

describe("tower refusals with their figures", () => {
  it("a curve too large names its tier and the largest curve it takes", () => {
    i18n.locale = "en";
    const d = "a curve may be at most half its chord and must stay within its tier's heights and not cross the tower's axis: tier 3 takes a curve of at most 0.412 m (tower_bulge)";
    expect(refusalReason(d)).toBe("the curve of tier 3 may be at most 0.412 m (half its chord, within its heights and not across the tower's axis)");
    i18n.locale = "cs";
    expect(refusalReason(d)).toContain("0,412 m");
  });
  it("a pitch no rise gives is refused with its own text (UX-010)", () => {
    i18n.locale = "en";
    expect(refusalReason("a pitch sets the rise of a tier that narrows (0° to 90°) or widens (90° to 180°) to a rise of at most 100 m (tower_tier_pitch)")).toContain("a pitch sets the rise only of a tier");
  });
  it("a save over the towers' tiers names that limit, not too many objects", () => {
    i18n.locale = "en";
    const d = describeError({ code: "too_many", detail: "a job has at most 120 tiers in all its towers; this change would make 126 (tower_tiers_limit)" });
    expect(d.message).toBe("All towers of a job have at most 120 tiers together; no more can be added.");
    expect(describeError({ code: "too_many", detail: "too many parts: 70 (at most 64)" }).message).not.toContain("120");
  });
});
