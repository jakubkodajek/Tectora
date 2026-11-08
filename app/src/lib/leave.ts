// Leaving the open job (Back, AI follow mode, a file opened from the system,
// closing the window) goes through the editor first, so edits that could not
// be saved are not thrown away without asking (ERR-001).

/**
 * The open editor's check: saves pending edits and returns whether the job
 * may be left. "ask" asks the user when the save failed; "quiet" (follow
 * mode, not started by the user) then just refuses.
 */
export type LeaveGuard = (mode: "ask" | "quiet") => Promise<boolean>;

let guard: LeaveGuard | null = null;
let unsaved: () => boolean = () => false;
let busyCheck: () => boolean = () => false;

/** Registered by the editor while it is open; null when it closes. */
export function setLeaveGuard(g: LeaveGuard | null, hasUnsaved: () => boolean = () => false, busy: () => boolean = () => false) {
  guard = g;
  unsaved = g ? hasUnsaved : () => false;
  busyCheck = g ? busy : () => false;
}

/** Removes `g` if it is still the registered guard (a newer editor may have replaced it). */
export function releaseLeaveGuard(g: LeaveGuard) {
  if (guard === g) setLeaveGuard(null);
}

/** Whether the open job may be left now (true when no editor is open). */
export function mayLeave(mode: "ask" | "quiet" = "ask"): Promise<boolean> {
  return guard ? guard(mode) : Promise.resolve(true);
}

/**
 * The user is in the middle of something that switching the screen would
 * throw away: drawing, placing an object, typing in a field, or a dialog
 * (the editor's own, or one on the library screen). Follow mode then only
 * offers to open the other job (UX-001).
 */
export function isBusy(doc: Pick<Document, "querySelector" | "activeElement"> = document): boolean {
  if (doc.querySelector('[role="dialog"]')) return true;
  if (guard) return busyCheck();
  return isTyping(doc.activeElement);
}

/**
 * The focused element holds typed text that is not stored yet. Fields driven
 * by `numeric` / `textfield` say so themselves (`data-typing`); any other
 * focused text field counts as typing.
 */
export function isTyping(a: Element | null): boolean {
  const tag = a?.tagName;
  if (tag !== "TEXTAREA" && tag !== "INPUT") return false;
  if (tag === "INPUT" && !["text", "search", "email", "url", "tel", "password", ""].includes((a as HTMLInputElement).type ?? "")) return false;
  const el = a as HTMLElement;
  if (el.hasAttribute?.("data-field")) return el.hasAttribute("data-typing");
  return true;
}

/** Edits that are not saved yet, for checks that cannot wait (page unload). */
export function hasUnsaved(): boolean {
  return unsaved();
}
