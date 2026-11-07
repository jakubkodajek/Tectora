# Project memory

Durable, version-controlled memory shared by Jakub and every AI agent. Agents
start each session with an empty context; this folder is how knowledge
survives between sessions, tools (Claude Code, Codex, Copilot) and people.

| File | What goes in | Who writes | Loaded |
| --- | --- | --- | --- |
| `project-context.md` | Product, users, constraints, glossary | Jakub; agents only on request | Every Claude session (imported by `CLAUDE.md`) |
| `lessons-learned.md` | Short imperative rules learned from mistakes | `tectora-docs-keeper`, humans | Every Claude session (imported) |
| `decisions.md` | One-line log of decisions, links to ADRs in `docs/adr/` | `tectora-docs-keeper`, `tectora-adr` | On demand |
| `progress.md` | Append-only session log and handoff notes | `tectora-docs-keeper`, `tectora-handoff` | Last entries at session start |
| `features.json` | Feature list with a `passes` flag, mirrors `ROADMAP.md` | Jakub adds items; agents only flip `passes` | On demand |

## Rules

- Keep the always-loaded files short (under ~60 lines each). Long instruction
  files reduce adherence (Claude Code docs, "How Claude remembers your
  project", https://code.claude.com/docs/en/memory).
- Facts that matter for part of the code go into `.claude/rules/` with
  `paths:` frontmatter; procedures go into skills.
- `ROADMAP.md` stays the human-readable plan; `features.json` is its
  machine-readable mirror with acceptance criteria. Keep both in step.
- `features.json` is JSON on purpose: Anthropic found models less likely to
  rewrite JSON inappropriately than Markdown task lists
  (https://www.anthropic.com/engineering/effective-harnesses-for-long-running-agents).
- Never store secrets, customer data or DACAD material here.
