---
name: tectora-adr
description: >-
  Writes an Architecture Decision Record for a Tectora decision that is hard
  to reverse: the file format, a new crate or dependency, the engine path,
  the UI stack, how AI clients connect. Use when such a decision is made or
  proposed, or when the user asks why something is built the way it is.
---

# tectora-adr

Decision: $ARGUMENTS

1. Next number: `ls docs/adr/ | sort | tail -1`.
2. Copy `docs/adr/template.md` to `docs/adr/NNNN-<kebab-title>.md`.
3. Fill in context, at least two options with pros and cons, the decision
   and its consequences. Name the source of every factual claim (DACAD
   case, docs page, benchmark, commit).
4. Status starts as `Proposed`; only Jakub sets `Accepted`.
5. Add one line to `memory/decisions.md` linking the ADR.
