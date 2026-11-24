// One rule for Gable then Eave in the wall bar and the context menu (UI-007),
// and the dormer refusals in the user's language (MATH-001, MATH-003).
import { describe, expect, it } from "vitest";
import { i18n } from "./i18n.svelte";
import { dormerRefusal, refusalReason } from "./errors";
import type { EdgeDef } from "./types";
import { eavePitchFor, pruneEavePitches, rememberEavePitch, useEavePitchJob } from "./walltype";

const wall = (x: number, y: number, pitch: number): EdgeDef => ({ start: { x, y }, pitch_deg: pitch, eave_height: 3, overhang: 0.5 });

describe("eavePitchFor", () => {
  it("brings back the wall's own pitch, else the nearest eave wall's, not the preset", () => {
    const es = [wall(0, 0, 35), wall(12, 0, 33), wall(12, 8, 35), wall(0, 8, 35)];
    // Never switched in this session: the previous wall's pitch.
    es[1] = { ...es[1], pitch_deg: 90 };
    expect(eavePitchFor(7, es, 1, 40)).toBe(35);
    // Switched to a gable from 33°: back to 33°.
    rememberEavePitch(7, wall(12, 0, 33));
    expect(eavePitchFor(7, es, 1, 40)).toBe(33);
    // A part of gables only takes the preset.
    const gables = [wall(0, 0, 90), wall(5, 0, 90), wall(5, 5, 90)];
    expect(eavePitchFor(8, gables, 0, 40)).toBe(40);
  });
});

describe("dormerRefusal", () => {
  it("says why the core refused a dormer, in the UI language", () => {
    i18n.locale = "cs";
    expect(dormerRefusal({ code: "dormer_below_roof", detail: "x" })).toMatch(/pod střechou/);
    expect(dormerRefusal({ code: "dormer_pitch", detail: "x" })).toMatch(/2°/);
    expect(dormerRefusal({ code: "internal", detail: "x" })).toBeNull();
  });

  it("names too many segments of a round dormer (HUNT-103)", () => {
    for (const [locale, word] of [["cs", "segment"], ["sk", "segment"], ["en", "segments"], ["de", "Segmente"]] as const) {
      i18n.locale = locale;
      expect(dormerRefusal({ code: "dormer_segments", detail: "x" })).toMatch(word);
    }
  });
});

describe("a helper's refused OK (ARCH-003)", () => {
  it("names the wall ranges, not a general reason", () => {
    i18n.locale = "en";
    const d = "the helper gives values a wall does not take (wall 0: overhang 57357 out of range (0 to 20 m)); check the lengths typed (m against cm or mm)";
    expect(refusalReason(d)).toMatch(/no wall takes/);
  });
});

describe("eave pitch memory per job (UI-104)", () => {
  it("does not carry a remembered pitch into another job with the same corners", () => {
    useEavePitchJob("job-a");
    rememberEavePitch(0, wall(12, 0, 35));
    useEavePitchJob("job-b");
    const es = [wall(0, 0, 40), { ...wall(12, 0, 0), pitch_deg: 90 }, wall(12, 8, 40), { ...wall(0, 8, 0), pitch_deg: 90 }];
    expect(eavePitchFor(0, es, 1, 30)).toBe(40);
  });

  it("forgets a wall's pitch once the wall is an eave again (the gable was undone)", () => {
    useEavePitchJob("job-c");
    rememberEavePitch(0, wall(12, 0, 35));
    // Undo: the wall is an eave at 35° again.
    pruneEavePitches([{ edges: [wall(0, 0, 40), wall(12, 0, 35), wall(12, 8, 40), wall(0, 8, 40)] }]);
    // An AI makes it a gable later; Eave takes the neighbour's pitch, not the old memory.
    const es = [wall(0, 0, 40), { ...wall(12, 0, 0), pitch_deg: 90 }, wall(12, 8, 40), wall(0, 8, 40)];
    expect(eavePitchFor(0, es, 1, 30)).toBe(40);
  });

  it("keeps the memory of a wall that is still a gable", () => {
    useEavePitchJob("job-d");
    rememberEavePitch(0, wall(12, 0, 33));
    const es = [wall(0, 0, 40), { ...wall(12, 0, 0), pitch_deg: 90 }, wall(12, 8, 40), wall(0, 8, 40)];
    pruneEavePitches([{ edges: es }]);
    expect(eavePitchFor(0, es, 1, 30)).toBe(33);
  });
});
