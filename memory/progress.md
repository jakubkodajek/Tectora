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

## 2026-11-08 - clear-canvas-and-face-pitch
- Done: editor "Clear all" (toolbar, confirm, undo; deleting the last part
  now clears too) + MCP `clear_roof` (`tectora_core::edit::clear_roof`);
  plan writes each face's pitch and area at its centre along its eave
  (`app/src/lib/facelabels.ts`), hidden with the dimensions toggle and while
  the breakdown shows face numbers.
- State: `make check` exit 0; `npx playwright test objects mcp editor` 63 passed.
  `make sweeps`: the 2 dark-theme accent contrast failures predate this change.
- Next: export of the plan picture and the German line list (other thread).
## 2026-11-08 · Export for the offer (plan sheet + price-sheet text)

- Done: `tectora_core::export`: the text list in the format of DACAD's
  "Liste" as TERRAN's Slovak-patched build writes it (verified against
  DACAD under Wine; `m˛` glyph and Slovak group names on purpose, the Excel
  price sheet parses them), and an A4 plan sheet (lengths, areas, pitches,
  totals) as SVG and a hand-written PDF. Export dialog in the editor (PDF,
  PNG, copy image, copy text), Tauri commands + ui_bridge, MCP
  `export_list` / `export_plan`.
- Known gaps: wall-area ("Stena") groups, dormers as their own roof number,
  one hole entry per face, DIN Norm group only for one material.
- Next: Jakub checks the pasted text in the real Excel sheet.
- Blockers / open questions: none.
## 2026-11-08 · Price-sheet text = TERRAN's DACAD list

- Done: the default text list is now byte-identical to the list TERRAN's
  DACAD wrote for a real job (L-shaped hip roof): only `STREŠNÉ PLOCHY`,
  `PLOCHY` (with the wall area `Stena : 1`, from the wall stage) and
  `LÍNIE`, totals without members; lengths and areas requantized through
  `%10.5f` like DACAD's SOL (32,313 m, not 32,314 m). The old full list
  pushed the faces past row 115, beyond what the Excel sheet sums; it is
  now `ListOptions::full` / MCP `export_list full` / "Detailed list" box.
  Checked in TERRAN's `.xls` under LibreOffice (sk locale): all 6 faces and
  the ridge/hip/valley/eave totals read.
- Open: `Stena : 1` for several roofs and the head line (DACAD writes the
  .dad path there; the sheet reads Partner/Realizátor/Koncový from it) are
  not verified against DACAD.
