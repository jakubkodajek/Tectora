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
