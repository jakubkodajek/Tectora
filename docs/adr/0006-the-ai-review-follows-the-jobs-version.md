# ADR-0006: The AI review follows the job's version, not a per-entry rule

- **Status:** Accepted by Jakub on 2026-11-24; its mechanism is superseded by ADR-0007 (Accepted)
- **Date:** 2026-11-24
- **Deciders:** Jakub Kodajek (to accept)

## Context

The app shows a review banner while AI changes wait for the user's Accept or
Revert; MCP `get_review`, the AI tab, the library's AI mark and the by-AI
filter show the same review. Until round 18 the core decided entry by entry
which AI changes "wait" (`store::waits`, `step_from`, `revert_effect`), and
the review's start and Revert all's starting point depended on what later
reverts named. Rounds 15 to 19 of the audit each patched that rule, and each
time a random mix of AI changes, AI reverts and the user's Revert / Undo found
a new shape: Revert all left an AI change in the job or took accepted work
out (audit 2026-11-24 ARCH-1802, HUNT-1803), work the user had reverted came
back with no banner (TEST-1801 t1906), and one Accept did not end the review
(ERR-1801). The per-revert walk was also quadratic on long logs (ARCH-1801).

Constraints: no change to the `.tectora` file, the change-log lines or
`.review.json` (an older Tectora on the same folder keeps working); the
editor autosaves the user's own edits without a log entry; the user's Undo of
Revert all must reopen the review (round 17); a Revert of one AI step keeps
the others waiting (round 16); nothing counts as accepted without Accept
(round 17).

## Options considered

1. **Patch the per-entry rule again** (judge an AI revert against the job at
   the review's start, send the newest shown entry on Accept). Small now; the
   verifier could not make t1906 pass with it on paper, and the same kind of
   bug returned five rounds in a row.
2. **Store a copy of the accepted job** and compare the file with it. Simple
   to state, but the editor's unlogged saves make the file differ for reasons
   that are not the AI's, the copy would have to be rebased on every editor
   save, and that rebase swallows the reopened review after an Undo of
   Revert all. Needs a new file.
3. **Versions from the log** (round 19 design A). Every logged entry leaves
   the job in a version: a change makes a new one, a revert goes back to the
   version before the oldest entry it covers. The user settles a version by
   Accept, their own change, or their revert to a settled version; an AI
   entry never settles. A review is open exactly while the current version
   is not the settled one because of the AI; Revert all reverts from the
   first entry after the settle point and lands on it. Computed on read from
   what the log already holds, one forward walk.

## Decision

We choose **option 3** because it answers one question about the job instead
of one per entry, so the invariants the random mixes check (Revert all ends
at the settled job, nothing accepted is taken out, one Accept ends the
review, an AI step that changes the settled job always waits) hold by
construction for logged operations, and it needs no new file. Owner decision
1 of round 19 (recommended option a, pending Jakub's answer).

## Consequences

- Positive: `waits`, `step_from` and the 1000-entry fallback are gone; one
  function (`store::review_of`) feeds the banner, the AI tab, MCP, the card
  and the filter; what reverts cover is read in linear work.
- Visible changes: an AI that takes a change out and puts it back exactly
  (or the reverse) leaves no banner; Accept accepts everything the editor has
  shown; `since_ts_ms` may name an AI revert that is no listed step; the
  user's own Undo that brings back an AI change they never accepted (one the
  sender of a bundle had reverted) opens the review.
- Negative / risks: with the user's unlogged edits a revert is a merge, so
  "version" is exact only for what the log records; a revert naming an entry
  no longer in the log is read by time, as before. The property test
  `t1905` includes an unlogged edit for this.
- Follow-ups: read `latest_undone`, `changed_by_ai` and the history's
  taken-out / put-back lists from the versions too, so one model decides all.

## Sources

- Audit 2026-11-24 round 19, `verify/verdicts-rules.md`, design A and owner
  decision 1; findings ARCH-1801, ARCH-1802, HUNT-1803, ERR-1801, TEST-1801.
- `crates/tectora-core/src/store.rs` tests `t1901`-`t1907`, `t19_*`.
