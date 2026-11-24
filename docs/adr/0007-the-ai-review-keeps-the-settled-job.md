# ADR-0007: The AI review keeps a copy of the job as the user last settled it

- **Status:** Proposed
- **Date:** 2026-11-24
- **Deciders:** Jakub Kodajek (to accept; owner decision 1 of audit round 20,
  recommended option a)
- **Supersedes:** the mechanism of ADR-0006 (option 3, versions from the log).
  ADR-0006's invariants stay; how they are kept changes.

## Context

ADR-0006 derived the review from the change log on every read: each entry
left the job in a version, and the user "settled" versions by Accept, their
own change, or a revert to a settled version. Round 20 of the audit found
three more shapes where that derivation is wrong, each in a different clause
of `store::review_of`:

- ARCH-1901 (S1): the log is trimmed to 50 entries. After about 47 AI entries
  in one review, an AI revert or the user's Revert all left AI changes in the
  job with no banner, and accepted work could be taken out with no trace
  (t1905 at 80 steps, seed 1008).
- UX-1901 / TEST-1901: the user's Undo of Revert all over an AI's revert of
  accepted work "settled" again (it landed on an ancestor version), so the
  AI's removal of accepted work stood with no banner (t2002).
- HUNT-1901: the listed step was the newest no-op AI revert, not the revert
  that took accepted work out, and Accept silently confirmed the loss.

It was the third round in a row with such shapes: every operation on the log
(trim, a revert naming a revert, a take-back, a received log) is a new case
for a rule that replays the log.

ADR-0006 had rejected "store a copy of the accepted job" for two reasons:
the editor's unlogged saves would have to be rebased onto the copy, and that
rebase would swallow the reopened review after an Undo of Revert all.

## Options considered

1. **X: keep the versions, protect the log.** Never trim entries of the open
   review or what its reverts name, bound the log's size, and patch the
   settle rule for take-backs and the listed step (four changes in
   `review_of` / `trim_log`). Cheap per save, but a log of 300 AI entries on
   a 100 KB job is about 60 MB, read on every card and save, and when the
   byte window cuts anyway the rule is wrong again.
2. **Y: keep the settled job in a file** (`.activity/<id>.baseline.json`)
   from the AI's first write until Accept. Rules R1-R9 (`store/baseline.rs`):
   R1 the AI's first write takes the job before it (an AI's new job: the job
   as created, and the creation waits); R2 later AI writes leave it; R3 the
   editor's unlogged saves and the user's put-back of the offer's figures are
   applied to it (three-way merge; a value the AI set and the user edited
   keeps the baseline's value and is remembered, so Revert all refuses with
   `revert_conflict`); R4 the user's own other logged change and the
   engine's recalculation settle (no baseline); R5 the user's revert from
   inside the review leaves it; R6 the user's revert from before it (Ctrl+Z
   past accepted work) makes the job it writes the baseline, R7 its take-back
   brings the old one back; R8 with no baseline, the user's revert that puts
   back an AI change which did not stand at the last Accept starts one; R9
   Accept drops it, or starts it again before an AI entry newer than the
   accepted time.
3. **Ask the user each time.** Not a rule.

## Decision

We choose **Y** because its two user-facing guarantees do not depend on the
log at all: **a review is open exactly while the baseline exists and the job
differs from it** (or the AI's creation waits), and **Revert all writes the
baseline**, logged as the user's revert from `since_ts_ms`, so Ctrl+Z takes it
back through the log and R5 keeps the review open. ADR-0006's two objections
are answered: the unlogged saves are R3 (merge.rs's three-way merge), and
reverts and take-backs do not rebase (R5), so Undo of Revert all reopens the
review. The random mixes that broke every log rule pass (t1905 at 80 steps
over 30 seeds, t2001 with user changes, AI take-backs, AI-created jobs,
Revert all followed by Undo and logs past the trim).

## Consequences

- Positive: the trim no longer matters (the log is cut to its newest 50
  entries, nothing kept in place); `review_of`, `versions` and `mark_chain`
  leave the review path (they stay for the one-time migration and for the
  revert counts); the banner's figures and `get_review.before` are baseline
  against now, so no step list can hide a change; Revert all of the banner
  names its review (`since_ts_ms`) and answers `revert_stale` once that
  review is over (ERR-1905); Duplicate and a new job from this roof copy a
  waiting review (owner decision 2 of round 20).
- Visible changes: when the user (or the AI) puts the job back exactly as
  settled, the banner goes by itself and a Revert all has nothing to write.
  `since_ts_ms` may name an entry the log no longer holds. The library card's
  AI mark then names the newest AI change of the history (no logged revert
  undid it).
- Existing test expectations changed, each because of the new rule (rule 4 of
  AGENTS.md: this ADR is the written decision, pending Jakub):
  - `r18_a_revert_of_changes_already_undone_by_hand_ends_the_review`: the
    review ends by itself, Revert all writes nothing, the card names the AI's
    change (was: the revert was logged, no card mark).
  - `t19_a_revert_that_moves_the_version_but_not_the_file_is_logged`, renamed
    `t19_a_users_edit_back_to_the_baseline_ends_the_review`: nothing is
    logged (was: a revert was logged).
  - `revert_all_after_more_ai_steps_than_the_log_keeps_restores_the_users_version`:
    `since_ts_ms` is the AI's first step, older than the kept log, and Revert
    all from it writes the user's version (was: the oldest kept entry stood
    for it).
  - `the_history_handed_out_is_what_the_next_save_keeps`: the history of a
    long AI log is its newest 50 entries; the review's start is in the
    baseline (was: the first waiting entry was kept in place).
  - `a_history_version_outside_the_limits_is_refused_with_its_own_code` and
    `an_out_of_limits_revert_over_a_damaged_file_does_not_call_it_fine`: the
    out-of-limits version is also put into the baseline file, which is what
    Revert all writes now; the codes and "nothing written" are unchanged.
- Negative / risks: while a baseline exists every save reads and writes a
  job-sized file (per card one small read). Two files are not written
  together: the baseline is written first and put back when the job write
  fails; a lost baseline write is caught up from the log on read (R1, R4).
  An older Tectora on the same folder does not apply R3: its unlogged saves
  would look like part of the review, so the baseline records a digest of
  the job as Tectora last wrote it and Revert all then answers
  `revert_outside` (Accept and the Revert of single steps still work); its
  logged entries are caught up on read. The file is untrusted input: read
  only as a regular file, never through a link, at most two jobs' size; a
  damaged one counts as missing.
- Migration: a job without a baseline file (written by an older Tectora) gets
  its review from the log once, by the round-19 rule, with what its Revert
  all would have written; the file is written at the next write (also by
  Accept), never by a read.
- Not carried: bundles (a received job arrives reviewed, owner decision of
  2026-11-23); the job's trash entry carries it, purge removes it.

## Sources

- Audit 2026-11-24 round 20, `verify/verdicts-rules.md` section 1 (design Y,
  rules R1-R9, the prototype run) and owner decisions 1 and 2; findings
  ARCH-1901, TEST-1901, TEST-1902, UX-1901, HUNT-1901, ERR-1905, UX-1905.
- `crates/tectora-core/src/store/baseline.rs`; store tests `t2001`-`t2004`,
  `arch1901_*`, `hunt1901_shape`, `y_*`, `t1905_long_mixes_cross_the_log_trim`.
