// Notices an editor that was left could not show, for the next editor of the
// same job in this window (audit ERR-902). Opening an offer recalculates its
// stored result once: when the user leaves the job while that runs, the
// answer reaches an editor that is gone, and the next open finds nothing to
// recalculate. The notice is kept here instead, in memory only (a restart
// forgets it; the result card and the history still say it).
import type { RefreshedEntry } from "./types";

const missed = new Map<string, RefreshedEntry>();

/** Keeps the recalculation notice of job `id` that its (left) editor did not show. */
export function keepMissedNotice(id: string, entry: RefreshedEntry) {
  missed.set(id, entry);
}

/** The notice of job `id` a left editor kept, taken once. */
export function takeMissedNotice(id: string): RefreshedEntry | null {
  const e = missed.get(id) ?? null;
  missed.delete(id);
  return e;
}
