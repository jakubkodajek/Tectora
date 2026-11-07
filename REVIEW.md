# REVIEW.md

Guidance for automated and human reviewers of Tectora changes. Read by the
`tectora-review` agents, the `code-review` plugin in CI, and people.

## What to flag, in priority order

1. **Roof results.** Any change in a roof, area, line length or DACAD line
   code that is not backed by a DACAD fixture or measured case; any edit of a
   fixture's expected output; any "tidying" of `dcore/`, `modell/` or
   `coresol/` arithmetic or ordering.
2. **Untrusted input.** Job files, bundles, settings and MCP calls reaching
   the core without id, size and coordinate-range checks; `unwrap`, slice
   indexing or unbounded loops on that data; ZIP path traversal; panics
   outside `model::guarded`.
3. **Data safety.** A save that can lose a job, its attachments or its
   history; a format change without a migration; newer files not refused.
4. **MCP parity.** A UI feature without its MCP tool; an MCP write without
   `summary`; a Tauri command missing from `examples/ui_bridge.rs`.
5. **Product rules.** DACAD, COIN or German DACAD terms in UI text; a string
   missing in one of cs, sk, en, de; colours outside the tokens in
   `app/src/app.css`; an un-awaited `window.confirm`; an API or path that
   works on only one of macOS and Windows.
6. **Tests.** Changed behaviour without a test that fails on the old code;
   tests deleted, ignored, loosened, or made non-deterministic.
7. **DACAD material.** Any `.DAD`, DACAD binary, DLL, help file, licence key
   or decompiled code in the diff.

## What not to flag

- Formatting (rustfmt owns it) and import order.
- Geometric oddities in a roof that is byte-identical to DACAD's: that is the
  rule, not a bug; a missing plausibility warning for it can be a finding.
- Style preferences no linter enforces, and issues with no concrete failing
  input.

## Severity

| | means |
| --- | --- |
| S1 | roof result wrong or differs from DACAD, data loss, crash or hang, a job cannot open, a security hole an untrusted file or MCP input can reach. Blocks merge. |
| S2 | a feature broken with no workaround, MCP cannot do what the UI does, a user can easily do the wrong thing without noticing. Fix before merge. |
| S3 | broken with a workaround, wrong or missing translation, layout broken at a supported size, confusing flow. Can follow up. |
| S4 | cosmetic, naming, small inconsistency. Report at most three. |

Every finding needs `file:line`, a concrete failure (input, expected,
actual), evidence (command output, failing test, screenshot) and a fix
suggestion. The same scale is used in `tectora-test` and the review board.
