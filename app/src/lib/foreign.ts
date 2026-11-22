// What the library's note says about entries in Tectora's own folders that
// block work (store.rs `foreign_entries`, audit 2026-11-22 ARCH-701 /
// UX-701): one sentence per kind, saying what is blocked and what to do,
// and per job for the kinds about one job.
import { t } from "./i18n.svelte";
import type { BrokenFile, ForeignItem, ForeignKind } from "./types";

const KINDS: ForeignKind[] = ["activity", "files", "trash", "exports", "job_files", "lock", "lock_fix", "new_lock"];

/** An entry as the app lists it; an older core sends no `kind`. */
export type Foreign = BrokenFile & Partial<Pick<ForeignItem, "kind" | "job">>;

/**
 * The note's sentences for `entries`, in the order of the kinds, each once
 * (a sentence about one job once per job). `name` gives a job's name for
 * its id. An entry without a known kind gets the general advice.
 */
export function foreignSentences(entries: Foreign[], name: (id: string) => string = (id) => id): string[] {
  const out: string[] = [];
  const add = (s: string) => {
    if (!out.includes(s)) out.push(s);
  };
  for (const kind of KINDS) {
    for (const e of entries) {
      if (e.kind !== kind) continue;
      add(t(`lib.foreign.${kind}`, { job: e.job ? name(e.job) : e.id }));
    }
  }
  if (entries.some((e) => !e.kind || !KINDS.includes(e.kind))) add(t("lib.foreignHint"));
  return out;
}
