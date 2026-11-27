// UX-1803 / UI-1803: the status bar while a review call is out names what
// it does; a Redo through the core read "Reverting…" in every language.
import { describe, expect, it } from "vitest";
import { DICTS } from "./i18n.svelte";
import { acceptTs, reviewStatusKey } from "./reviewstatus";

describe("reviewStatusKey", () => {
  it("names Undo and Redo as such, Revert and the put-back as before", () => {
    expect(reviewStatusKey("undo")).toBe("editor.undoing");
    expect(reviewStatusKey("redo")).toBe("editor.redoing");
    expect(reviewStatusKey("revert")).toBe("editor.reverting");
    expect(reviewStatusKey("revertAll")).toBe("editor.reverting");
    expect(reviewStatusKey("restoreOffer")).toBe("editor.restoringOffer");
  });
  it("has its texts in every language, and Redo never reads as reverting", () => {
    for (const [locale, d] of Object.entries(DICTS)) {
      for (const what of ["undo", "redo", "revert", "restoreOffer"] as const) expect(d[reviewStatusKey(what)], `${locale} ${what}`).toBeTruthy();
      expect(d[reviewStatusKey("redo")], locale).not.toBe(d["editor.reverting"]);
      expect(d[reviewStatusKey("undo")], locale).not.toBe(d["editor.reverting"]);
    }
  });
});

// Audit 2026-11-24 TEST-2012: Accept was sent with the history read's newest
// entry while the banner came from a newer review read, so the AI's put-back
// the banner showed stayed waiting and the banner came back.
describe("acceptTs", () => {
  it("goes up to the newest step of the review shown, past an older history read", () => {
    expect(acceptTs(127835, 126113, [127168, 128098])).toBe(128098);
    expect(acceptTs(null, 126113, [128098])).toBe(128098);
  });
  it("keeps everything shown when the history read is the newest", () => {
    expect(acceptTs(130000, 126113, [128098])).toBe(130000);
    expect(acceptTs(null, 126113, undefined)).toBe(126113);
    expect(acceptTs(120000, 126113, [])).toBe(126113);
  });
});
