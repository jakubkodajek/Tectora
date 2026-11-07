---
paths:
  - "crates/*/tests/**"
  - "tests/fixtures/**"
  - "app/src/**/*.test.ts"
---

# Rules for tests and fixtures

- Expected outputs in `tests/fixtures/` come from DACAD only. Never
  regenerate or edit one to match Tectora; a changed fixture is a human
  decision written in the PR.
- Never delete, skip, `#[ignore]` or loosen an assertion to make a change
  pass. Known DACAD-identical oddities are ignored tests with the reason in
  the attribute, reported by the nightly `core-stress` workflow.
- One behaviour per test; the name says the behaviour or the DACAD case.
- Deterministic: fixed proptest seed, no network, no wall clock, scratch
  folders (`TECTORA_CONFIG_DIR`, a temp library), never the user's files.
- Tests that need private DACAD samples read `DACAD_SAMPLES` and are
  ignored without it. Never commit DACAD material.
- A bug fix starts with a test that fails on the old code.
