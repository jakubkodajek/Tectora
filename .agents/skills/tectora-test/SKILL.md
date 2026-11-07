---
name: tectora-test
description: >-
  Tests a Tectora change end to end and reports bugs in a fixed format: core
  fixtures against DACAD, analytic and property tests, UI unit tests, MCP
  parity with the editor, a click-through in the browser preview, and the
  macOS / Windows specifics. Use when the user says "test it", "find bugs",
  "QA", "does it work", "check before merge", "is the roof right", or after a
  feature lands.
---

# tectora-test

## Step 1: the automated gate

Run all of it; paste the result lines into the reply or the PR.

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
cd app && npm run check && npm test && npm run build && cd ..
python3 .claude/skills/tectora-i18n/sweep.py
python3 .claude/skills/tectora-design/contrast.py
```

What each layer proves:

| layer | where | proves |
| --- | --- | --- |
| DACAD fixtures | `crates/tectora-core/tests/*_cases.rs`, `tests/fixtures/` | same output as DACAD, byte-exact for core files |
| analytic, invariants | `tests/analytic.rs`, `tests/invariants.rs` (when present) | closed-form roofs within 1 mm; plan area / cos(pitch) etc. |
| properties | `tests/properties.rs` (`PROPTEST_CASES`, nightly 20 000) | no panic or hang on random input |
| schema | `tests/schema.rs` | `docs/tectora.schema.json` matches the model |
| UI unit | `app/src/lib/*.test.ts` (vitest) | calc helpers, transforms, translations |

DACAD's private sample files are not in the repo. Tests that need them read
`DACAD_SAMPLES` and are ignored without it; say in the report whether they ran.

## Step 2: the plan

For the changed feature, number the cases: `F-H1` happy path, `F-E1` edge,
`F-N1` negative. Go down this list:

- roof: gable, hip, half-hip, shed, L / T / U, several parts touching, pitch 0°
  and 90°, zero-length direction walls, overhang 0, very large and very small
  buildings, values typed with a comma
- edit: undo / redo, revert an AI change, save and reopen, a file from an older
  schema version, a damaged file
- AI: the same operation over MCP gives the same project as in the editor
  (**MCP parity**), the change shows live with a summary, Accept and Revert work
- UI: all four languages, light and dark, window 600x420, keyboard only
- platform: paths with spaces and diacritics (`Zakázky/Nový Dům`), Windows
  reserved names in job names, macOS and Windows file associations

## Step 3: click through

Run the browser preview and walk the cases (`tectora-ui-preview`), with
screenshots as evidence. Native parts (file dialogs, attachments, double-click
open, installers) need `npx tauri dev` or a CI build on macOS and Windows:
list them for the user as a short checklist when you cannot run them.

## Step 4: report bugs

One block per bug, only for what you reproduced. "Might be a problem" goes in a
separate "to check" list.

```
### BUG-001: <what is wrong>
- Severity: S1 | S2 | S3 | S4
- Case: F-E2   Build: <commit>   Platform: preview | macOS | Windows
- Steps: 1. … 2. …
- Expected: …   Actual: …
- Evidence: screenshot path, failing test name, console text
- Status: open | fixed in <sha> | won't fix: <reason>
```

| | means |
| --- | --- |
| S1 | roof result differs from DACAD, data loss, crash or hang, a job cannot open |
| S2 | a feature broken with no workaround, MCP cannot do what the UI does |
| S3 | broken with a workaround, wrong text or language, layout broken at a supported size |
| S4 | cosmetic |

## Step 5: fix loop

S1 and S2 first. Each fix starts with a failing test (core test, vitest, or a
fixture), then the fix, then the whole gate again.

## Output

The gate results, the case table with pass / fail, bugs by severity, and what
could not be run here.
