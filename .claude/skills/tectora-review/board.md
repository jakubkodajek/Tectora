# Review board protocol

Every Tectora reviewer agent (`tectora-ux-reviewer`, `tectora-ui-reviewer`,
`tectora-math-reviewer`, `tectora-arch-reviewer`) follows this protocol. The
agents start with clean context and never see the conversation that launched
them; the board is the only thing they share.

## Where

One folder per review run, under the gitignored `target/`:

```
target/review/<run>/            # <run> = YYYY-MM-DD-<short sha>, given in the prompt
  ux.md   ui.md   math.md   arch.md     # one file per agent, only its owner writes it
  shots/                                 # screenshots, optional
  summary.md                             # written by whoever runs the review afterwards
```

If the prompt gives no run folder, use `target/review/latest/`.

Each agent writes **only its own file**. Agents run in parallel, so editing
another agent's file would lose writes. To tell another agent something, put
it in your own file under "Notes for other reviewers"; the others read every
file on the board.

## When to read the board

1. At the start, after reading the project rules: read all other files in the
   run folder (they may be empty or missing; that is fine).
2. Halfway through, and again just before finishing: read them again. When a
   note is addressed to you, check it and answer in your own file (confirmed,
   not reproduced, or out of scope, with evidence).
3. If someone already reported the same defect, do not file a duplicate: add a
   line "Also seen: <their ID>, <what you add>" under your related finding, or
   only in your notes.

## Format of your file

```markdown
# <AREA> review, <run>, commit <sha>

Scope: what you covered. Not covered: what you could not run and why.

## Findings

### UX-003: <one line: what is wrong>
- Severity: S1 | S2 | S3 | S4
- Confidence: confirmed | likely | to check
- Where: path/to/file.rs:123 (or screen + steps)
- Evidence: command and output, test name, screenshot path, quoted code
- Why it matters: who is hurt and how
- Fix: the smallest change that fixes it
- Related: MATH-001, ARCH-004   (optional)

## Notes for other reviewers
- @math: <question or hand-off, with the place to look>
- @arch: …

## Answers to notes
- re ui.md @ux note 2: confirmed, see UX-005.
```

Write the file as you go (create it in the first minutes, append findings as
you confirm them), so the others can see your progress while they run.

IDs: `UX-`, `UI-`, `MATH-`, `ARCH-` (security findings use `ARCH-` with
"security" in the title), numbered from 001 in your file.

## Severity

| | means |
| --- | --- |
| S1 | roof result wrong or differs from DACAD, data loss, crash or hang, a job cannot open, a security hole an untrusted file or MCP input can reach |
| S2 | a feature broken with no workaround, MCP cannot do what the UI does, a user can easily do the wrong thing without noticing |
| S3 | broken with a workaround, wrong or missing translation, layout broken at a supported size, confusing flow |
| S4 | cosmetic, naming, small inconsistency |

## Evidence rules

- A finding is **confirmed** only if you reproduced it: a command with its
  output, a failing test you wrote (keep it in `target/review/<run>/tests/`,
  not in the source tree), a screenshot, or code quoted with its line number
  plus the input that reaches it.
- "Might be a problem" is `to check`, never `confirmed`.
- Never claim DACAD does or does not do something without a fixture, a test
  or a note in the code that says so. Say "unverified against DACAD".

## Hard limits for every reviewer

- **Read-only on the source tree.** Do not edit, format, commit, push or
  delete anything outside `target/review/<run>/`. The fixes are someone
  else's job; your job is the evidence.
- No network calls to outside services, no installing global tools beyond
  what the repo's own commands install (`npm ci` in `app/` is fine).
- Never print or copy secrets (env files, keys, tokens) into the board.
- Never commit or quote DACAD material (`.DAD`, binaries, decompiled code,
  help text beyond a short term).
