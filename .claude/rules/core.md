---
paths:
  - "crates/tectora-core/src/**"
  - "crates/tectora-mcp/src/**"
  - "crates/tectora-cli/src/**"
---

# Rules for the roof core and its callers

- `dcore/`, `modell/` and `coresol/` are faithful ports of DACAD: keep float32
  reads, DACAD's tolerances, evaluation order, list order and first-match
  rules. Do not tidy, reorder or "simplify" them; line codes hinge on
  knife-edge cases. Read `dcore/PORTING.md` before changing a stage.
- A rule change in the roof logic needs DACAD evidence (fixture, DACAD.HLP,
  a measured case) named in the commit or the PR.
- A roof identical to DACAD's stays as DACAD gives it, even when it is
  geometrically odd; add a plausibility warning in `check.rs` instead.
- Errors that DACAD also ends with are `engine::Failure::Dacad`; anything
  else is `Failure::Ours` and falls back to the sweep engine. Do not blur
  the two.
- Everything reachable from a job file, bundle, Tauri command or MCP call
  runs inside `model::guarded` after the limits check: no `unwrap`, slice
  indexing or unbounded loop on untrusted data.
- An editor operation lives in `edit/` and is called by both the Tauri
  command and the MCP tool; every MCP write takes a `summary`.
- A model change updates `docs/tectora.schema.json`
  (`TECTORA_WRITE_SCHEMA=1 cargo test -p tectora-core --test schema`) and a
  format-version change adds a migration on load.
- `cargo fmt --all --check` stays clean; add no new `cargo clippy --workspace --all-targets` warnings (33 existed on 2026-11-07, most of them `neg_cmp_op_on_partial_ord` in `edit/calc.rs`).
