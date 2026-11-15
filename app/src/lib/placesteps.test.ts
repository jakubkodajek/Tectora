import { describe, expect, it } from "vitest";
import { DICTS } from "./i18n.svelte";
import { placeSteps, type PlaceKind } from "./placesteps";

describe("placeSteps (UX-012)", () => {
  const kinds: PlaceKind[] = ["dormer", "chimney", "window", "hole", "line", "face"];
  it("names the tool and its next click, in every language", () => {
    for (const k of kinds) {
      for (const n of [0, 1]) {
        const s = placeSteps(k, n);
        for (const l of Object.keys(DICTS) as (keyof typeof DICTS)[]) {
          expect(DICTS[l][s.title], `${l} ${s.title}`).toBeTruthy();
          expect(DICTS[l][s.text], `${l} ${s.text}`).toBeTruthy();
        }
      }
    }
  });
  it("is not the wall bar's tip and moves on after a line's first point", () => {
    expect(placeSteps("face", 0)).toEqual({ title: "tool.face", text: "plan.placeFace", counted: true });
    expect(placeSteps("line", 0).text).toBe("plan.placeLine");
    expect(placeSteps("line", 1).text).toBe("plan.placeLineEnd");
    expect(placeSteps("chimney", 0).counted).toBe(false);
  });
});
