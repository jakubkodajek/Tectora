// UX-1803 / UI-1803: the status bar while a review call is out names what
// it does; a Redo through the core read "Reverting…" in every language.
import { describe, expect, it } from "vitest";
import { DICTS } from "./i18n.svelte";
import { reviewStatusKey } from "./reviewstatus";

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
