# Progress log

Append-only. Newest entry at the bottom. Read the last 2-3 entries at
session start.

Template:

```
## YYYY-MM-DD - <slug>
- Done: what changed (commits, PR)
- State: make check green/red; branch; PR
- Next: the single next concrete step
- Blockers / open questions:
```

## 2026-11-07 - state-at-agent-setup

Snapshot of the repository when the agent setup was added, so the first
sessions that use it know where things stand.

- Done before: Tectora 2 on `main` (#15), Settings screen (#10), localized
  errors (#9). The roof core is DACAD's own core (`dcore`) with the
  Modellverschnitt for joined roofs; see `ROADMAP.md` "Done".
- Open PRs against `main` (2026-11-07): #16 signed releases and in-app
  updates, #17 property counterexample triage, #18 AI change review in the
  library, #19 library of past roofs, #20 edit roof objects in plan and 3D
  (#25 stacked on it: roof windows in 3D), #21 read technical drawings over
  MCP and score drawing-to-roof builds, #22 E2E tests for every button,
  #23 warning for merged in-line walls, #24 calculation breakdown, #26
  reviewer agents and `tectora-review`.
- State: on `main` + #26, `cargo test --workspace --exclude tectora-app`
  green (Linux container without WebKitGTK), `npm run check` 0 errors,
  `npm test` 56 passed. `contrast.py` reports 2 failing pairs (accent on
  surface 4.21:1, accent on accent-soft 3.70:1), which is why `make sweeps`
  is not part of `make check` yet. CI (`desktop.yml`) does not run
  `cargo fmt --check` or clippy; `make lint` does. `cargo fmt --check` is
  clean; clippy gives 33 warnings (25 in `edit/calc.rs`), not denied.
- Next: fix the two contrast pairs, then add `sweeps` to `make check`.
- Blockers / open questions: GitHub Actions on Jakub's account failed
  before any step in `dev_template` on 2026-11-07 (likely billing or
  minutes); check that the new `agent-config` workflow runs here.

## 2026-11-07 - agent-setup
- Done: AGENTS.md (canonical) + CLAUDE.md imports, build-pipeline agents
  (planner, test-engineer, implementer, bug-hunter, docs-keeper), skills
  `tectora-feature`, `tectora-bug-hunt`, `tectora-handoff`, `tectora-adr`,
  path rules, hooks, `memory/`, `docs/architecture.md`,
  `docs/agent-workflow.md`, ADRs 0001-0003, REVIEW.md, Makefile,
  `scripts/sync_agents.py` with Codex/Copilot copies, `agent-config` CI.
  Branch `claude/agent-memory-setup`, stacked on #26.
- State: see the PR for the `make check` output.
- Next: merge #26, then this PR; run `tectora-feature` on the next ROADMAP item.
- Blockers / open questions: none.

## 2026-11-08 - ai-connect-and-mcp-audit
- Done: one-click Connect for Claude Desktop (incl. Windows MSIX copies),
  Codex and Antigravity (`tectora_core::assistants`, ported from Buzzdome);
  MCP parity fixes: update_edges `clad`/`direction_deg`/`all` keeps gables,
  edit_walls move_point carries direction changes, transform_items `align`.
  PR #29.
- State: `make check` exit 0; `npx playwright test mcp` 27 passed.
  `app/src-tauri/src/lib.rs` not compiled here (no WebKitGTK).
- Next: test Connect on Windows with the MSIX Claude Desktop; tectora-review.
- Blockers / open questions: none.
