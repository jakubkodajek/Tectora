# Decision log

One line per decision, newest last. Significant or hard-to-reverse decisions
get an ADR in `docs/adr/`. Format: `- YYYY-MM-DD: <decision> - <reason> [ADR-NNNN] (commit/PR)`

Entries before 2026-11-07 were recorded retroactively from the git history.

- 2026-06-23: First Tectora as a Python (PySide6, PyVista) app with its own straight-skeleton roofs - quick start (b64812a). (superseded 2026-10-26)
- 2026-10-26: Rewrite as a Rust workspace with a Tauri 2 + Svelte desktop app; core in `tectora-core`, shared by app, CLI and MCP - one engine for every client, native macOS and Windows installers [ADR-0002] (e0f665f, f1d5471)
- 2026-10-26: Job library is a plain folder of `.tectora` JSON files with an activity log - app, CLI and MCP server can work on the same jobs at once; AI changes are reviewable (4e6c7bb)
- 2026-10-27: Port DACAD's Modellverschnitt faithfully, quirks included, for roofs with two or more parts - line codes depend on its exact float32 behaviour (0877d89)
- 2026-10-28: DACAD's own core (`dcore`, a 1:1 port checked against an emulator) produces every roof; Tectora's sweep engine is only the fallback for failures of ours - results must equal DACAD's, including DACAD's errors [ADR-0003] (a897d97)
- 2026-11-02: Every editor operation lives in `tectora_core::edit` and has an MCP tool; every MCP write carries a `summary` - AI must be able to do all the UI does, visibly (906a792, 1e04e63)
- 2026-11-02: File format has a JSON Schema and versions with migrations on load; newer files are refused - old jobs must keep opening, newer ones must not be damaged (906a792)
- 2026-11-02: No DACAD import or export in the product; `.DAD` reading only in dev tools - DACAD files are third-party data (911032e)
- 2026-11-02: Roofs that are byte-identical to DACAD stay unchanged even when geometrically odd; a localized plausibility warning is shown instead (fdd283e, `check.rs`)
- 2026-11-02: Goal for AI: build the roof from technical drawings over MCP, a colleague only reviews and accepts it in the app (ROADMAP.md, section "AI")
- 2026-11-03: Old Python app removed from the repository - Tectora 2 replaces it fully (6003380)
- 2026-11-07: `AGENTS.md` is the canonical agent instruction file, imported by `CLAUDE.md`; agents and skills live in `.claude/` and are generated for Codex and Copilot - one source for every AI tool [ADR-0001]
- 2026-11-07: Feature work runs through the planner, test-engineer, implementer, reviewers, verifier and docs-keeper agents (`tectora-feature`) - separation of duties and evidence before merge (`docs/agent-workflow.md`)
