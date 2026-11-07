---
name: tectora-review
description: >-
  Runs a full, deep review of Tectora with four reviewer subagents that each
  start with clean context (UX, buttons and UI controls, roof calculations,
  architecture and security) and share findings on one review board, then
  merges them into one prioritised bug list. Use when the user says "review
  the whole app", "deep review", "audit the project", "find bugs everywhere",
  "run the reviewers", or names one area: "UX review", "check all buttons",
  "check the calculations", "security review", "architecture review".
---

# tectora-review

Four agents in `.claude/agents/`, each run as a subagent with fresh context:

| agent | area | board file |
| --- | --- | --- |
| `tectora-ux-reviewer` | whole-app user journeys, wording, flow | `ux.md` |
| `tectora-ui-reviewer` | every button and control, states, keyboard, i18n, contrast, layout, E2E | `ui.md` |
| `tectora-math-reviewer` | roof math, DACAD parity, quantities, breakdown, numerics | `math.md` |
| `tectora-arch-reviewer` | module boundaries, MCP parity, errors, security | `arch.md` |

They share nothing but the repository and the board in
`target/review/<run>/` (protocol: `board.md` next to this file). Each writes
only its own file and reads the others', so a hand-off ("@math: check this
area") reaches the right agent without the agents seeing each other's
context.

## Step 1: set up the run

```bash
RUN="$(date +%F)-$(git rev-parse --short HEAD)"
mkdir -p "target/review/$RUN/shots" "target/review/$RUN/tests"
```

Note the branch and commit. If the browser preview is needed (UX and UI),
the agents start it themselves; two agents can share one preview, so start
it once first (`cd app && npm run preview` in the background) and tell both
that it is running on http://localhost:1430.

## Step 2: round 1, in parallel

Launch all four in one message with the Agent tool, `subagent_type` set to
the agent name. The prompt carries only facts, not conclusions, so each one
reads the code without your bias:

> Review run folder: target/review/<RUN>. Commit <sha> on <branch>.
> Scope: the whole project (or: <area / PR / files>). The browser preview is
> running on http://localhost:1430 (or: not running; start it if you need it).
> Follow .claude/skills/tectora-review/board.md. Go deep: this is a full
> audit, not a skim.

For one area only, launch just that agent.

## Step 3: round 2, cross-check

When all four are done, every board file has notes for the others that may
have arrived after the addressee last looked. Send each agent one more
message (SendMessage to the same agent, so it keeps its own context but
still not yours):

> Round 2. Read all files in target/review/<RUN> again. Answer every note
> addressed to you, check findings of the others that touch your area, and
> mark duplicates with "Also seen". Then finish as before.

Skip round 2 if no file has notes for another reviewer.

## Step 4: merge

Read the four files yourself and write `target/review/<RUN>/summary.md`:

1. One table of all findings, S1 first: ID, severity, confidence, one line,
   where, owner area. Merge duplicates under the first ID and list the others.
2. Cross-area chains: a finding in one area caused by one in another (for
   example a wrong number in `math.md` that the UX reviewer saw as a
   confusing result).
3. What was not covered, from every file's "Not covered".
4. A proposed fix order: S1 and confirmed first, each with the test that
   should prove the fix.

Do not fix anything in this skill. Report to the user: counts by severity,
the top findings, and the path to `summary.md`. Fixes go through the normal
workflows (`tectora-test` fix loop, `tectora-dacad-parity` for core changes),
each one starting with a failing test.

## Keeping the agents honest

- Each finding needs evidence (board.md "Evidence rules"). Drop or downgrade
  to "to check" anything without it when you merge.
- Agents are read-only on the source tree. If one edited a source file,
  revert it (`git status` after the run) and say so.
- A finding that contradicts a project rule in `CLAUDE.md` (for example
  "change the core to be geometrically exact") is not a finding; note it as
  rejected with the rule.
