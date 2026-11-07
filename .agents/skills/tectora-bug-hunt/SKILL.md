---
name: tectora-bug-hunt
description: >-
  Adversarial bug hunt on one Tectora area (job files, bundles, MCP, a roof
  feature, the CLI, the library): the bug hunter breaks it with reproducible
  inputs, verifiers confirm, and every confirmed bug gets a failing
  regression test. Use when the user asks to stress-test, harden, fuzz or
  "try to break" something. For a code-reading audit use tectora-review.
---

# tectora-bug-hunt

Target: $ARGUMENTS

1. Slug `bughunt-<area>`; `mkdir -p target/work/<slug>`.
2. Build once so the agent does not wait on it:
   `cargo build -p tectora-cli`.
3. Launch `tectora-bug-hunter` with the target and the slug. Ask for at
   least 10 attack ideas and a reproduction for every one that failed.
4. For each reproduced S1/S2 bug launch `tectora-finding-verifier` with the
   bug text and the REPRODUCE lens (S1: add a second one with IMPACT).
   Drop what is not confirmed into a "to check" list.
5. For each confirmed bug launch `tectora-test-engineer` to add a failing
   regression test.
6. Report the confirmed bugs, most severe first, each with its reproduction
   command and the new test name. Ask whether to fix them now (then
   `tectora-implementer`) or to open issues.
