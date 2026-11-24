/** A review call the editor waits for (Editor.svelte `runReview`). */
export type ReviewPress = "revert" | "revertAll" | "restoreOffer" | "undo" | "redo";

/**
 * The status bar's text key while review call `what` is out: an Undo or a
 * Redo through the core says so, not "Reverting…" (a Redo puts the AI's
 * change back; audit 2026-11-24 UX-1803, UI-1803).
 */
export function reviewStatusKey(what: ReviewPress): "editor.reverting" | "editor.restoringOffer" | "editor.undoing" | "editor.redoing" {
  switch (what) {
    case "restoreOffer":
      return "editor.restoringOffer";
    case "undo":
      return "editor.undoing";
    case "redo":
      return "editor.redoing";
    default:
      return "editor.reverting";
  }
}

/**
 * The time Accept is sent with: everything the editor has shown, the newest
 * entry of the history read (`seenTs`), the newest listed step and every
 * step of the review that drew the banner. The review read can be newer than
 * the history read; an Accept below a step the banner shows leaves that step
 * waiting and brings the banner back (audit 2026-11-24 TEST-2012).
 */
export function acceptTs(seenTs: number | null, pendingTs: number, steps: number[] | undefined): number {
  return (steps ?? []).reduce((m, s) => Math.max(m, s), Math.max(seenTs ?? pendingTs, pendingTs));
}
