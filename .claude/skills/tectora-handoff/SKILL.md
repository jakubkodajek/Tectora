---
name: tectora-handoff
description: >-
  Ends a Tectora work session cleanly: a clean or explained working tree, the
  gate result, and a progress entry, decisions and lessons in memory/ so the
  next session (human or agent) continues without the conversation. Use when
  the user says they are done, before the context runs out, or after
  finishing a task.
---

# tectora-handoff

1. `git status` and `make check`. Commit work that passes; describe exactly
   what is unfinished otherwise. Check `git status` shows no DACAD material.
2. Launch `tectora-docs-keeper` with: what was done this session, the slugs
   used, the `make check` result, open questions, and the PR number if any.
3. Check the new `memory/progress.md` entry answers: what changed, state
   (green or red, branch, PR), the one next concrete step, and blockers.
4. Show the user the entry and the list of changed memory files.
