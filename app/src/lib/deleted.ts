// A job the editor deleted (the review banner's Delete job) is handed to
// the library, which says so with the same notice and Restore action as its
// own delete (UX-010): one place raises that notice and its gated action
// (audit 2026-11-24 UX-1703, UI-1702).
import type { TrashEntry } from "./types";

let handed: { gone: TrashEntry; name: string } | null = null;

/** The editor deleted `gone` (named `name`) and is leaving for the library. */
export function handOverDeleted(gone: TrashEntry, name: string) {
  handed = { gone, name };
}

/** The job the editor handed over, once. */
export function takeDeleted(): { gone: TrashEntry; name: string } | null {
  const h = handed;
  handed = null;
  return h;
}
