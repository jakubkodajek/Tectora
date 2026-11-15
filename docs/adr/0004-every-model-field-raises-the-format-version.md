# ADR-0004: Every model change raises the file format version

- **Status:** Proposed
- **Date:** 2026-11-15
- **Deciders:** Jakub Kodajek (to accept)

## Context

Jobs are `.tectora` files in a folder that several Tectora versions can
share: the in-app update leaves an older app beside a colleague's newer one.
`SCHEMA_VERSION` was raised only when a field changed meaning. Fields were
added to version 2 without a bump (`parts[].height` in 0.8.0 after 0.7.0
shipped as version 2). The model has no catch-all field, so a release that
does not know a field drops it on its next save without a word (review
ARCH-003: 0.7.0 opening a 0.8.0 job loses the building height).

## Options considered

1. **Keep unknown keys on every struct** (like `Settings::extra`) - a
   release would write back what it cannot read. Needs a catch-all field on
   `Project`, `RoofPart`, `EdgeDef` and about ten more structs, which every
   struct literal in the code and the tests must then name; and an older
   release that keeps a key it does not understand can still write a file
   whose parts disagree with it (a wall inserted by the older release shifts
   which wall the kept key belongs to).
2. **Raise the version with every model change** - an older release refuses
   the newer file (`newer_format`, the existing error) and never rewrites
   it. Cost: after a release that adds a field, colleagues must update
   before they can open the newer files.
3. Both.

## Decision

Option 2 as the rule, plus the cheap part of option 1: `Project::extra`
keeps unknown top-level keys (bounded by `MAX_EXTRA_BYTES`, scanned by the
range check, never read by the core). Version 3 is raised now, so 0.7.0 and
0.8.0 refuse files this release saves instead of dropping fields.
`Project::to_json` always stamps the current version.

The rule is enforced by a test: `tests/schema.rs`
`every_model_change_has_a_format_version` hashes the names and types of the
committed schema and compares it with `tests/fixtures/schema_versions.json`
for `SCHEMA_VERSION`; a model change without a new version and a new entry
fails. Doc comments do not count.

Nested catch-all fields (option 1 below the top level) were not added: see
the risk above, and the many struct literals in the DACAD fixture tests.

## Consequences

- Positive: a field can no longer be dropped silently by the previous
  release; the test makes the bump impossible to forget.
- Negative / risks: every model change forces colleagues on the shared
  folder to update before opening newer files; unknown keys inside parts,
  walls and objects are still lost when an older release of the same format
  version saves (only possible if the test is bypassed).
- Follow-ups: decide whether nested unknown keys are worth the catch-all
  fields; settle the wording of the `newer_format` message for the
  "update Tectora" case.

## Sources

- `target/review/2026-11-14-f5d9531/arch.md`, ARCH-003.
- `crates/tectora-core/src/model.rs` (`SCHEMA_VERSION`, `migrate`).
