# Lessons learned

Short imperative rules learned the hard way. Add one when the same mistake
happens twice or a review finding would have been prevented by a rule.
Newest first. Format: `- YYYY-MM-DD: <rule> (why: <one line, with the commit or PR>)`

- 2026-11-08: Number fields only through `use:numeric`, never native `type="number"`; commit before the press that changes the selection (why: typed values were lost or landed on the next wall in WebKit; e7e6b5f, `numeric.test.ts` guards it).
- 2026-11-08: When fixing a bug a user hit, add its class and guard test to `.claude/skills/tectora-review/edge-cases.md` (why: the same input / second-building / 3D-gable bugs reached the user one after another).
- 2026-11-08: Turn core polygons into geometry from their full extent, never from the first edge (why: a gable cut by a second roof starts with a vertical edge and was drawn skewed; 426516a).
- 2026-11-07: Run `make check` and show its result lines before calling anything done (why: agents tend to declare success early; the gate is cheap).
- 2026-11-03: When adding a field to the project, check it against `check_limits`; scan only input geometry, not metadata (why: 23a7ac6, the stored result's timestamp made every non-draft job "out of range").
- 2026-11-02: Validate the size of every number from a file before it reaches the core (why: 298c06a, x = -1e308 from the JSON fuzz passed validation and panicked in the polygon clipper; now `model::MAX_COORD`).
- 2026-11-02: Before calling a roof deviation a bug, run it through the DACAD core emulator (why: 298c06a, all 73 "wrong" roofs from the property tests were byte-identical to DACAD).
- 2026-11-02: Use one term per concept in every language and keep German DACAD labels out of the UI (why: 66b7030 had to unify zakázka/návrh and replace literal DACAD labels; the i18n test now enforces it).
- 2026-11-02: Keep `.DAD` reading in dev tools only (why: 911032e removed DACAD import from the app, CLI and MCP; the reader lives in `examples/support/`).
- Always: `await window.confirm` under Tauri (why: with the dialog plugin it returns a Promise, which is always truthy; rule in `AGENTS.md`).
