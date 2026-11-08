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

/** Registered by the editor while it is open; null when it closes. */
export function setLeaveGuard(g: LeaveGuard | null, hasUnsaved: () => boolean = () => false) {
  guard = g;
  unsaved = g ? hasUnsaved : () => false;
}

/** Removes `g` if it is still the registered guard (a newer editor may have replaced it). */
export function releaseLeaveGuard(g: LeaveGuard) {
  if (guard === g) setLeaveGuard(null);
}

/** Whether the open job may be left now (true when no editor is open). */
export function mayLeave(mode: "ask" | "quiet" = "ask"): Promise<boolean> {
  return guard ? guard(mode) : Promise.resolve(true);
}

/** Edits that are not saved yet, for checks that cannot wait (page unload). */
export function hasUnsaved(): boolean {
  return unsaved();
}
