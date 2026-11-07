---
name: tectora-review
description: >-
  Runs a full, deep review of Tectora with six reviewer subagents that each
  start with clean context (UX, buttons and UI controls, roof calculations,
  architecture and security, tests, silent failures), share findings on one
  review board, cross-check each other, and have every serious finding
  independently verified before it reaches the user. Use when the user says
  "review the whole app", "deep review", "audit the project", "find bugs
  everywhere", "run the reviewers", or names one area: "UX review", "check
  all buttons", "check the calculations", "security review", "architecture
  review", "test gaps", "error handling".
---

# tectora-review

The design follows Anthropic's own review tooling
(https://github.com/anthropics/claude-plugins-official/tree/main/plugins):

- `code-review`: independent reviewers in parallel, only high-signal issues,
  then a separate subagent validates each issue and unvalidated ones are
  dropped.
- `pr-review-toolkit`: specialised agents (code-reviewer with a 0-100
  confidence score reported from 80, silent-failure-hunter, pr-test-analyzer,
  type-design-analyzer, …).
- `claude-security`: map the codebase first, account for every top-level
  directory, and have verifiers with different lenses try to refute each
  finding, defaulting to false positive.
- Anthropic Engineering, "How we built our multi-agent research system"
  (https://www.anthropic.com/engineering/multi-agent-research-system): give
  each subagent an objective, an output format, tool guidance and clear
  boundaries, and let it write its work to files and return a short
  reference.

## The agents (`.claude/agents/`)

| agent | area | board file |
| --- | --- | --- |
| `tectora-ux-reviewer` | whole-app user journeys, wording, flow | `ux.md` |
| `tectora-ui-reviewer` | every button and control, states, keyboard, i18n, contrast, layout, E2E | `ui.md` |
| `tectora-math-reviewer` | roof math, DACAD parity, quantities, breakdown, numerics | `math.md` |
| `tectora-arch-reviewer` | module boundaries, MCP parity, schema, security | `arch.md` |
| `tectora-test-reviewer` | behaviour no test would catch, weak tests | `test.md` |
| `tectora-error-hunter` | swallowed errors, panics, silent UI failures | `errors.md` |
| `tectora-finding-verifier` | tries to disprove one finding through one lens | (returns a verdict) |

They share nothing but the repository and the board in
`target/review/<run>/` (protocol: `board.md` next to this file). Each
reviewer writes only its own file and reads the others', so a hand-off
("@math: check this") reaches the right agent without the agents seeing each
other's context or yours.

## Effort

Scale to the ask: one area → that one reviewer plus verification; a PR →
the reviewers whose area the diff touches, scoped to the diff; "whole app" or
"deep" → all six. Say which you chose.

## Step 1: set up the run

```bash
RUN="$(date +%F)-$(git rev-parse --short HEAD)"
mkdir -p "target/review/$RUN"/{shots,tests,verify}
```

Note the branch and commit. If the browser preview is needed (UX, UI, error
hunter), start it once (`cd app && npm run preview` in the background) and
tell the agents it is running on http://localhost:1430, so they do not start
competing copies. Build once (`cargo build --workspace --exclude tectora-app`
in the cloud container) so the agents do not all wait on the first build.

## Step 2: round 1, in parallel

Launch the reviewers in one message with the Agent tool, `subagent_type` set
to the agent name (in a session that started before these files existed, use
`general-purpose` and tell it to follow `.claude/agents/<name>.md`). The
prompt carries facts, not conclusions, so each reads the code without your
bias:

> Review run folder: target/review/<RUN>. Commit <sha> on <branch>.
> Scope: <whole project | area | diff of PR #n>. The browser preview is
> running on http://localhost:1430 (or: not running). Follow
> .claude/skills/tectora-review/board.md. Go deep: this is a full audit,
> not a skim. Do not modify the source tree.

## Step 3: round 2, cross-check

When all are done, send each reviewer one more message (SendMessage to the
same agent: it keeps its own context, still not yours):

> Round 2. Read all files in target/review/<RUN> again. Answer every note
> addressed to you, check findings of the others that touch your area, mark
> duplicates with "Also seen", and move anything below confidence 80 to
> "To check". Then finish as before.

Skip it if no file has notes for another reviewer.

## Step 4: verify

For every S1 and S2 finding (and any S3 the person will act on), launch
`tectora-finding-verifier` agents in parallel, each with the finding text as
written on the board, the repository path, the commit and one lens:

- S1: three verifiers, lenses REPRODUCE, REACHABILITY, IMPACT (security
  findings) or REPRODUCE, IMPACT, RULES (others).
- S2: two verifiers, REPRODUCE and RULES.
- S3: one verifier, REPRODUCE.

A finding is **verified** when a majority says CONFIRMED. Its severity is the
lowest that a confirming verifier supports (verifiers can lower severity,
never raise it). Not verified → "To check" with the verifiers' reasons.
Save the verdicts in `target/review/<RUN>/verify/verdicts.md`.

## Step 5: merge

Write `target/review/<RUN>/summary.md`:

1. One table of verified findings, S1 first: ID, severity, confidence, one
   line, where, area, verifier votes. Merge duplicates under the first ID.
2. Cross-area chains (a wrong number in `math.md` that `ux.md` saw as a
   confusing result, a missing test in `test.md` for a bug in `errors.md`).
3. Coverage: what each reviewer examined and set aside, and what nobody
   could run here (native dialogs, installers, macOS / Windows).
4. "To check": unverified or low-confidence items in one line each.
5. Fix order: S1 first, each with the test that should prove the fix.

Do not fix anything in this skill. Report to the person: counts by severity,
the top findings, and the path to `summary.md`. Fixes go through the normal
workflows (`tectora-test` fix loop, `tectora-dacad-parity` for core changes),
each starting with a failing test.

## Keeping the agents honest

- Every finding needs evidence (board.md "Evidence rules").
- Agents are read-only on the source tree. Run `git status` after the run; if
  one edited a source file, revert it and say so.
- A finding that contradicts a project rule in `CLAUDE.md` (for example
  "change the core to be geometrically exact") is rejected with the rule.
