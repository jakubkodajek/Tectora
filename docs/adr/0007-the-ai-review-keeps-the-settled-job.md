# ADR-0007: The AI review keeps a copy of the job as the user last settled it

- **Status:** Proposed
- **Date:** 2026-11-24
- **Deciders:** Jakub Kodajek (to accept; owner decision 1 of audit round 20,
  recommended option a; amended by round 21, owner decisions 1, 3, 4 and 5,
  recommended options, pending)
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
   swaps the two baselines (amended in round 21, below); R8 with no baseline, the user's revert that puts
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

## Amendments of round 21 (audit 2026-11-24, commit fca71c3)

Status unchanged: Proposed, for Jakub to accept with the round-20 text.

- **R10: another program's unlogged save is the colleague's edit**
  (ERR-2001, ARCH-2002; owner decision 1, option a). Round 20 refreshed the
  job's digest on every write without comparing it first, so an older
  Tectora's save followed by one more Tectora write (an AI step, a per-step
  revert, an editor save) was taken out by Revert all without a word, and an
  older Tectora's *logged* AI write was refused as an outside save. Now one
  function (`fold_outside`) runs on every read of the baseline and before
  every write sets a new digest: when the file is not what Tectora last
  wrote, entries another program logged since are R2 for their own change
  and a gap before their `before` is an unlogged save; what is left is
  applied to the baseline like R3, from the job Tectora last wrote (the
  file now keeps the job of an unlogged write, `last_saved`, so this base
  is always known for files of this version; for a logged write the log's
  entry is the base). The colleague's edit therefore stays through Revert
  all, an edit of a value the AI set is a conflict, and after the per-step
  reverts the job is its baseline and the review closes. Only when the base
  is not known (a baseline file of round 20 after an editor save) the
  baseline is marked `outside` and Revert all answers `revert_outside`
  until Accept, R4 or R6; nothing is lost silently.
- **An older Tectora's Accept is read from the review mark** (ARCH-2001).
  A Tectora without baseline files accepts by writing the review mark only.
  The baseline file now records the mark it was written with
  (`accepted_ms`); a newer mark on read is R9 with that time, in memory,
  and the next write persists it (the same pure rule as our own Accept, so
  applying it twice gives the same baseline). Not "mark newer than
  `since_ts_ms`" (R8 opens reviews whose start is at or below the mark) nor
  "newer than `log_ts_ms`" (the editor accepts with an entry's own time).
  A file of round 20 (no `accepted_ms`) is not checked.
- **R7 is a swap** (ARCH-2003): the take-back of a revert past accepted work
  brings the old baseline back and keeps the one it replaced, under the
  take-back's own time, so Undo, Redo, Undo, Redo alternate "no review" and
  "review" any number of times.
- Also: a remembered conflict ends when the user puts the AI's value back or
  makes the place the baseline's again (`conflict_was`, ERR-2002); a
  baseline file another job left at the id counts as missing (the log is
  empty or starts with a creation after the baseline's start, ARCH-2006);
  Revert all of an AI's new job whose creation the trim took out counts
  from the first kept step (ARCH-2005); the file is at most three jobs'
  size (`3 * MAX_JOB_FILE + MAX_SMALL_FILE`).
- The banner's change line and the plan's marks are `review_status.changes`,
  the baseline against the job now (owner decision 4); `before_rev` tells
  the editor when the baseline moved; the library card's `review_pending`
  feeds a "waits for review" mark and filter, also in MCP `list_projects`
  (owner decision 3).
- Test expectations changed by these amendments (rule 4 of AGENTS.md; this
  ADR is the written decision, pending Jakub):
  - `y_revert_all_refuses_after_another_programs_unlogged_save`, renamed
    `y_revert_all_keeps_another_programs_unlogged_save`: Revert all writes
    and keeps the other program's job number, and so does the Revert of the
    single step (was: Revert all answered `revert_outside`). The refusal is
    kept for the case it still applies to,
    `r10_an_outside_save_that_cannot_be_told_apart_is_refused_and_sticks`.
  - `y_an_out_of_order_log_lists_what_its_revert_left`: the first Revert all
    writes, because the file is the job of the log's newest entry (was:
    `revert_outside`, then the test saved the job once more and Revert all
    wrote). The listed steps and the job written are unchanged.
  - `y_undo_past_accepted_work_and_its_redo_keep_the_review_right`: extended
    (Undo, Redo, Undo, Redo and the banner's Revert all); no assertion
    changed.
  - `t2001` (random mixes): extended with another program's unlogged save
    (invariant: nothing takes out a value no AI entry wrote) and an Accept
    up to the first shown step (the later ones still wait); the oracle's
    "job without unlogged edits" also leaves out the name, which only that
    save writes. No check was removed or loosened.
  - `err.revert_outside` (all four languages) ends with "revert the AI's
    steps one by one in the AI tab, or accept them" (was: "accept or revert
    the AI's changes one by one", but Accept is not per step).

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
  - E2E `audit-r18-a.e2e.ts` "a Revert of a change the user already typed
    back ends the review (ERR-1701)", renamed "typing the AI's change back
    by hand ends the review (ERR-1701, ADR 0007)": the banner goes by itself,
    there is no Revert to press (was: the Revert logged a revert).
- Negative / risks: while a baseline exists every save reads and writes a
  job-sized file (per card one small read). Two files are not written
  together: the baseline is written first and put back when the job write
  fails; a lost baseline write is caught up from the log on read (R1, R4).
  An older Tectora on the same folder does not apply R3: its unlogged saves
  are folded in by R10 from the digest of the job as Tectora last wrote it,
  and only an edit that cannot be told apart makes Revert all answer
  `revert_outside` (Accept and the Revert of single steps still work); its
  logged entries and its Accept are caught up on read. The file is
  untrusted input: read only as a regular file, never through a link, at
  most three jobs' size; a damaged one counts as missing.
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
- Audit 2026-11-24 round 21, `verify/verdicts-rules.md` section 1 (1a-1e) and
  section 4, owner decisions 1, 3, 4 and 5; findings ERR-2001, ERR-2002,
  ARCH-2001 to ARCH-2006, UX-2001, UX-2003, TEST-2002, TEST-2005; store tests
  `r10_*`, `arch2001_*` to `arch2006_*`, `err2002_*`, `ux2001_*`,
  `t2101`-`t2105`.
