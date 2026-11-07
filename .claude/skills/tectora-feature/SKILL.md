---
name: tectora-feature
description: >-
  Builds one Tectora feature with the full agent pipeline: plan, failing
  tests first, implementation across core / MCP / app / i18n, a scoped
  review by the tectora-review agents plus the bug hunter, independent
  verification, a bounded fix loop and a memory handoff. Use when the user
  asks to build, add or change a feature, or names an item from ROADMAP.md
  or memory/features.json.
---

# tectora-feature

Task: $ARGUMENTS

You are the **orchestrator**. Keep your own context small: delegate the
heavy reading to subagents and keep only their handoff reports. Protocol,
roles and formats: `docs/agent-workflow.md`.

Small fixes (one function, no new behaviour) do not need this: implement,
run `make check`, and run `tectora-review` on the diff once. The pipeline
costs many more tokens than one session (Anthropic measured about 15x for
multi-agent work, https://www.anthropic.com/engineering/multi-agent-research-system).

## 0. Set up

- Pick a kebab-case `<slug>`; `mkdir -p target/work/<slug>`.
- If the task is an item of `memory/features.json`, use its id; otherwise
  tell the user which item you would add (humans add items).
- Branch from the current default: `git switch -c feat/<slug>`.
- Read the last entries of `memory/progress.md`.

## 1. Plan

Launch `tectora-planner` with the slug, the task text and pointers (ROADMAP
line, features.json id, DACAD case names the user mentioned). Read the plan.
If its open questions change the design, or roof logic has no DACAD
evidence yet, ask the user before going on.

## 2. Tests first

Launch `tectora-test-engineer` with the slug. Check that the report shows the
new tests failing for the right reason.

## 3. Implement

Launch `tectora-implementer` with the slug. Require the `make check` result
lines in its report.

## 4. Review (parallel, scoped to the diff)

In one message, launch:

- `tectora-bug-hunter` (always),
- the `tectora-review` reviewers whose area the diff touches, with the diff
  as scope and a run folder `target/review/<slug>-r<round>/` following
  `.claude/skills/tectora-review/board.md`:
  - roof logic, quantities, `edit/` → `tectora-math-reviewer`
  - crate boundaries, MCP, file format, bundles, Tauri, input handling →
    `tectora-arch-reviewer`
  - any Svelte or i18n change → `tectora-ui-reviewer` (and
    `tectora-ux-reviewer` for a new flow)
  - always → `tectora-test-reviewer` and `tectora-error-hunter`

## 5. Verify

Run the verification step of `tectora-review` (verifier lenses by severity)
over the board **and** `target/work/<slug>/findings-bugs.md`.

## 6. Fix loop (at most 3 rounds)

While verified S1 or S2 findings remain:
1. `tectora-test-engineer` writes a regression test per finding.
2. `tectora-implementer` fixes only the verified findings.
3. Re-run step 4 for the reviewers whose findings were fixed, then step 5.

After round 3, stop and give the user the remaining findings.

## 7. Finish

- Run `make check` yourself (and `make sweeps` for UI changes) and show the
  result lines.
- For UI changes, a screenshot from the browser preview
  (`tectora-ui-preview`).
- Launch `tectora-docs-keeper` with the slug and all reports.
- Tell the user: what was built, the evidence, the remaining S3/S4 or
  unverified items, and a PR title. Open the PR when the user asked for one;
  never merge.
