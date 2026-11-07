# ADR-0003: DACAD's own core, ported 1:1, is the roof engine

- **Status:** Accepted (recorded retroactively on 2026-11-07; in effect since 2026-10-28)
- **Date:** 2026-10-28
- **Deciders:** Jakub Kodajek

## Context

Tectora must give the same roofs, areas and line lengths as DACAD, the
software it replaces, because offers are made from these numbers. Tectora
first had its own weighted straight-skeleton sweep engine (`roof.rs`,
`union.rs`, `multiroof.rs`), tuned case by case against DACAD's outputs.
In October 2026 DACAD's core was ported stage by stage (S0-S6) and checked
against an emulator of the original (`crates/tectora-core/src/dcore/PORTING.md`),
together with the Modellverschnitt that joins several roofs (`modell/`).

## Options considered

1. **Own geometry engine, matched to DACAD by fixtures** - code we fully
   understand; but every new case can differ, and DACAD's knife-edge
   decisions (float32 tolerances, evaluation order) are hard to imitate.
2. **1:1 port of DACAD's core and Modellverschnitt** - byte-identical
   intermediate files (RD2, SOL, INT, W3D) and DACAD's own errors; cost:
   code that must not be "tidied", and roofs that are geometrically odd
   where DACAD is odd.

## Decision

Option 2 (a897d97, `engine.rs`). The engine writes the project as DACAD's
RD2, runs the ported core, and joins several roofs in the Modellverschnitt.
An error DACAD also ends with (`Failure::Dacad`) is the result. Only a
failure of ours (`Failure::Ours`: an RD2 the core cannot read, a stage the
port lacks) falls back to the sweep engine.

## Consequences

- Positive: results equal DACAD's, including its errors; verification is
  mechanical (fixtures, emulator dumps).
- Negative / risks: DACAD's geometrically wrong roofs are reproduced too
  (`docs/core-verification.md`, findings 2 and 3); the app shows a localized
  plausibility warning (`check.rs`) instead of changing them. `dcore/` and
  `modell/` must keep DACAD's arithmetic exactly (`.claude/rules/core.md`).
- Possible follow-up (a proposal, not decided): remove the sweep fallback once no `Failure::Ours` path is
  reachable from valid projects.

## Sources

- `crates/tectora-core/src/engine.rs` module docs, `dcore/PORTING.md`,
  `docs/core-verification.md`; commit a897d97 (2026-10-28).
