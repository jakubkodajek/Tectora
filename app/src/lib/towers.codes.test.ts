// The core's new refusal codes and tower findings in the user's language
// (audit ERR-004, ERR-005).
import { describe, expect, it } from "vitest";
import { i18n } from "./i18n.svelte";
import { arcReason, describeCalcError, dialogRefusal, refusalReason } from "./errors";
import { towerRows, warningText } from "./results";
import type { CalcResult, Project } from "./types";

describe("tower and arc refusal codes", () => {
  it("names the tower limit, a missing tower and a tower without a roof", () => {
    i18n.locale = "en";
    expect(refusalReason("a job has at most 50 towers (tower_limit)")).toBe("a job has at most 50 towers; no further tower can be added");
    expect(refusalReason("tower 3 does not exist (tower_missing)")).toContain("no longer in the job");
    expect(refusalReason("the tower cannot be calculated (tower_not_calculated)")).toContain("no tower roof");
  });

  it("names a wall, corner or building the outline no longer has", () => {
    i18n.locale = "en";
    expect(arcReason("wall 9 does not exist (arc_no_wall)")).toContain("wall is no longer");
    expect(arcReason("corner 7 does not exist (arc_no_corner)")).toContain("corner is no longer");
    expect(arcReason("part 2 does not exist (arc_no_part)")).toContain("building is no longer");
  });

  it("a dialog shows the core's code and text only for a reason it has no text for", () => {
    i18n.locale = "en";
    expect(dialogRefusal({ code: "refused", detail: "a job has at most 50 towers (tower_limit)" })).toEqual({ text: "A job has at most 50 towers; no further tower can be added", detail: null });
    expect(dialogRefusal({ code: "refused", detail: "something new" })?.detail).toBe("refused: something new");
    expect(dialogRefusal({ code: "internal", detail: "panicked" })).toBeNull();
  });
});

describe("a tower's shaft named instead of a chimney", () => {
  it("the calculation error and the no-area warning name the tower", () => {
    i18n.locale = "en";
    const p = { parts: [], towers: [{ label: "Lantern" }] } as unknown as Project;
    const d = describeCalcError({ kind: "non_convex_opening", message: "", towers: [0] }, p);
    expect(d.title).toContain("Lantern");
    expect(d.hint).not.toContain("Chimneys");
    expect(describeCalcError({ kind: "non_convex_opening", message: "" }, p).title).toContain("chimney");
    expect(warningText({ kind: "no_roof_area", value: 0, towers: [1] })).toContain("Tower 2");
  });

  it("a tower over a building without a roof is not on the ground and has no cladding figure", () => {
    const r = { parts: [], totals: {} as never, codes: {}, problems: [], towers: [{ label: "", result: { top_height: 1, base_perimeter: 4, base_area: 1, tiers: [], lines: [] }, roof_area: 1, wall_area: 0, shaft: { bottom: [], wall_area: 0, on_roof: false, base_below_roof: false, roof_failed: true } }] } as CalcResult;
    expect(towerRows(r)[0]).toMatchObject({ on_ground: false, roof_failed: true });
  });
});
