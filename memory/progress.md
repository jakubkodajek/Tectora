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

- Done: the text list is the list TERRAN's DACAD writes for the price
  sheet, byte-identical for a real job (L-shaped hip roof): only
  `STREŠNÉ PLOCHY`, `PLOCHY` (wall area `Stena : 1` from the wall stage,
  then material totals) and `LÍNIE` (totals only); lengths and areas
  requantized through `%10.5f` like DACAD's SOL (32,313 m, not 32,314 m).
  The old long list (all sections, members) is gone on Jakub's request: it
  pushed faces past row 115, beyond what the Excel sheet sums. Dormer faces
  no longer overflow the face name (were `wall_offset + usize::MAX`).
  Test `every_kind_of_roof_exports_a_list_the_price_sheet_reads` runs 34
  roofs (all examples × dormer, chimney + window, clad gables + metal +
  manual face; two buildings; flat) through the sheet's formulas.
- Open, not verified against DACAD: dormer face numbering (`Z<parts+1>.nn`),
  `Stena` for joined roofs and with dormers, the `Škridla - -Norm` line when
  windows make the DIN area differ, the head line (DACAD writes the .dad
  path; the sheet reads Partner/Realizátor/Koncový from it).
## 2026-11-08 - building-height
- Done: "Výška budovy (okap)" field in the roof definition bar (selected
  building; empty with "různá" when its walls differ) and MCP
  `set_part_height`, both `tectora_core::edit::height::set_part_height`:
  every wall at the eave height, its breaks and half-hip moved by the same
  amount, dormers / chimneys / hand-drawn lines on the building by the
  building's shift. Pure input, no roof math changed.
- State: `make check` exit 0, `make sweeps` clean; e2e joined, fields, mcp,
  i18n, editor passed.
- Next: Jakub tries it on the two-hip-roof job from his screenshot.
## 2026-11-08 - input-and-deep-audit (branch claude/project-thread-6k6mz9)
- Done: number fields via `use:numeric` (typed values kept on a click
  elsewhere, decimal comma, bounds); Building vs Manual face told apart and
  "make it a building"; joined roofs readable (dashed lines under a higher
  roof, "joined to" from the core, `covered` warning); 3D gable from the
  polygon's full extent. Deep audit with the six reviewers (73 findings,
  board target/review/2026-11-08-ebf4a0c-deep/summary.md), verified, all
  fixed in seven branches merged here; re-check 67 fixed + 6 partly, the
  partly ones and an autosave/AI save race fixed afterwards. Edge-case
  classes 1-9 in `.claude/skills/tectora-review/edge-cases.md`.
- State: `make check` exit 0, full E2E green (see PR).
- Next: DACAD runs for window_bigger_than_face, rows/grid_4x2 and a dormer
  off the roof (review tests/); Jakub decides count limits, trash restore
  button, `after` in the activity log, P16 test expectation.
- Blockers: app/src-tauri is compiled only by CI (no WebKitGTK here).
## 2026-11-08 · DACAD input limits

- Done: core input problems for pitch outside 2-178°, negative eave,
  pitch break below the eave or twice at one height (`EdgeDef::problems`,
  `MIN_PITCH_DEG`/`MAX_PITCH_DEG`); MCP `create_project` defaults and
  `update_edges` refuse them; roof bar bounds; cs/sk/en/de messages.
- State: `make check` exit 0; e2e `editor fields mcp errors` 77/78, the
  failing `MCP parity › building height` also fails without this change
  (PR #39 base).
- Next: gap analysis doc lists the other DACAD gaps; Jakub chose only
  limits + building height for now.

## 2026-11-09 · Resize any chimney or roof opening

- Done: the object bar edits every chimney / roof opening through its box
  along the first side (`openingBox` / `reshapeOpening`, port in
  `tectora_core::edit::openings`); before, a chimney placed on a slanted
  wall, aligned, rotated or mirrored lost its size fields ("edit in job
  file"). MCP `update_opening` (chimney, roof opening, window) for parity.
- State: `make check` exit 0, `make sweeps` pass, preview screenshot in
  project files `chimney-resize-2026-11-09/`.

## 2026-11-10 · Roof windows in the export

- Done: list gets DACAD's `STREŠNÉ OKNÁ` section and one deduction row
  per window (`Z1.01/k`) and per chimney/opening (`Z1.01:k`, slope
  length); gross `-Norm` row dropped (DACAD default norm is none); plan
  sheet draws windows, chimneys, openings with their deductions, dormer
  footprints and hand-drawn faces/lines. Evidence: DACAD runs P3a/P3c
  under Wine (PR #43).
- State: `make check` exit 0.
- Next: window with a corner on a hip/ridge is not deducted (review
  ERR-010); `KOMÍN / OTVORY` list section; Jakub to try a paste with a
  window in the real price sheet.
- 2026-11-10 (later): plan-sheet labels (line lengths, cut-out areas, face
  labels) keep clear of every line and label; a window whose corner lies
  exactly on a ridge/hip/valley is deducted again (review ERR-010). DACAD
  evidence: window records sit under a wall and DACAD takes w×h off that
  wall's face wherever the window lies (moved-record runs under Wine).

## 2026-11-10 · dormer window (branch claude/project-thread-j9381s)

- Done: DACAD-style dormer window (type tiles, plan, side section) opens when
  a dormer is placed and from the dormer bar ("Upravit vikýř"); front wall
  height above the roof, depth and ridge length set the eave height; roof
  pitch, slope lengths and rise follow. Core `edit::dormers::{section,
  eave_for, host_slope}` (closed form, checked against DACAD's dialog numbers
  for a shed dormer and against the calculated roof for all 7 kinds), TS
  mirror parity-tested, Tauri/bridge `dormer_host`, MCP `section` and
  `wall_height`/`depth`/`ridge_length`.
- State: `make check` exit 0, `make sweeps` clean.
- Next: Jakub's dad tries it; DACAD's dialog also has separate front and
  side overhangs and a back width for the trapezoid dormer (not ported, one
  overhang as before).

## 2026-11-10 · dormer overhangs and trapezoid back width (same branch)

- Done: separate front overhang (front wall) and side overhang (cheeks) in
  `DormerParams` (`overhang`, `side_overhang`, read back from the edges, so
  old jobs keep their one overhang), the dormer window (front overhang in the
  section, side overhang in the plan), the dormer bar and MCP
  (`overhang` sets both, `front_overhang` / `side_overhang` apart). Trapezoid:
  `section.back_width` (shed roof width where it meets the roof) and
  `pitch_for_back_width` / `solve` set the cheek pitch from it. The section
  of a trapezoid whose shed roof meets the roof before the cheeks close now
  takes its depth from the shed plane (it wrongly assumed a ridge before).
  All checked against the calculated roof (depth, ridge, back width) for 4
  overhang pairs.
- Evidence: DACAD dialog resources (Schleppgaube 1259 side / 1265 front
  overhang; Trapezgaube 1245 top, 1247 base, 1248 cheek pitch) and the
  decompiled dialog init (type 2 = Trapezgaube, needs top < base width).
  Not verified with a DACAD run: whether DACAD's trapezoid puts the cheek
  eave lower than the front (its picture shows a trapezoid front wall).
- State: `make check` exit 0, sweeps clean.

## 2026-11-10 · whole-app architecture, UI and UX review (branch claude/project-thread-tnm6jz)
- Done: review with ux and arch reviewers on Fable 5.2, ui and error
  hunter on Opus 5.5, five verifiers (one lens each); board
  `target/review/2026-11-10-8cd1db0-ux/` (70 findings, 17 of 18 S1/S2
  verified). Fixed: ARCH-053 (pitch-break / dormer-wall limits), ERR-005
  (torn change-log line), ARCH-043 (dialog:allow-save), ERR-007/008 (MCP
  part index and unknown keys refused), ERR-004, UI-002, UX-011, UX-013,
  ERR-003, UI-001, UI-019, UX-017.
- State: `make check` exit 0; full E2E 211 passed; new `guards.e2e.ts`.
- Next: Jakub decides the clarity redesign (wall bar "More", one term
  "Budova", Back vs undo, copy for Excel in Results), ARCH-052 (ask before
  an AI moves the jobs folder) and the core edit-operation refactor
  (ARCH-042/045/046). Roof windows (ERR-001/002/010) are in their own thread.

## 2026-11-10 — clarity redesign and AI permission prompts (PR #45, round 2)

- Done (Jakub's decisions 1-5): wall bar shows only everyday fields, the
  rest behind "Více"; one term "Budova" ("Budova 1", "Budova 2", older
  default names shown the same); "Zpět" only for undo, labelled delete
  buttons, dormer delete asks; Results has "Kopírovat pro Excel" (bytes
  identical to the export list) and an "On the roof" list with deductions;
  an AI client's folder change is a request the user allows in the app
  (`settings.folder_request`, Tauri `answer_folder_request`); MCP tools carry
  readOnly/destructive annotations and Connect AI explains how to stop
  Claude and Antigravity asking at every step.
- State: `make check` exit 0 (UI 197 tests), `make sweeps` exit 0, full E2E.
- Open for Jakub: paste the copy into the real TERRAN price sheet once (the
  sheet is not in the connected folders); should core/MCP default part names
  become "Budova N"; a core field for deduction totals; `openings.ts` still
  picks the window face with its own probe instead of the core's.

## 2026-11-14 - building height, roof placements, dormer flicker, join fixes (branch claude/project-thread-syyw67)
- Done: building height separate from eave height (`RoofPart.height`,
  eaves may sit lower); distances from eave and side for every roof object
  (`edit/position.rs`, MCP `place_item`, plan dimension lines); bigger
  dialogs and Properties by double-click / right-click with Cancel/OK;
  dormer 3D flicker fixed; joined buildings: walls under 1 cm are a
  `zero_length_wall` problem, a building the core fails on alone no longer
  takes the other roofs (`without_failing`), Modellverschnitt edge budget
  stops the OOM on slightly turned wings. Six reviewers + two verifiers on
  board `target/review/2026-11-14-cd283f7/`; all confirmed findings fixed.
  MCP `calc` reports `problems`, exports refuse a job with a left-out or
  failing building.
- State: `make check` exit 0 (Rust 370 passed, UI 217), E2E placement 6/6.
- Next: Jakub reviews the PR; decide on the departure from DACAD's
  all-or-nothing run (decisions.md 2026-11-14). Open review items in the PR.
- 2026-11-14 (fix plan `lowered-eave`, review board `2026-11-14-9d5d68c`):
  departures from the core's result now run as one chain in
  `openings::generate_valid` (merged planes, lean-to wing, sweep), each only
  where the core stops and only after `check::sound`; objects and a far
  second building no longer change the roof, turned buildings give the same
  figures, hipped jog / L / wing keep the standard roof, a departed roof
  has a wall area, cut and sweep budgets stop the hang on combs.
- State: `make check` exit 0 (Rust 410 passed, UI 219), `make sweeps` pass.
  Reproducers: err001 141 jobs all calculate (42 with
  `not_calculated_by_core`); fuzz 209 jobs, 137 calculate, 72 keep the
  core's error (no departure passes the self-check).
- Open: two or more low wings and L-shaped low arms keep the error (one
  cut per building); departures only for buildings standing clear of the
  others; a dormer left off a departed roof gets no warning;
  `tests/invariants.rs` now skips departed roofs (needs Jakub's sign-off).

## 2026-11-14 - closed 3D buildings and a whole-app sweep (branch claude/project-thread-14l1th)
- Done: 3D walls go from the ground to the roof's underside over the wall
  line (`scene3d.wallUnderRoof`), which closes the gap under the roof and
  the stacked walls of a raised building height; stacked dormers keep their
  own walls. Review run `target/review/2026-11-14-f5d9531/` (UX, UI, arch
  and security, tests, silent failures; roof math left to the core thread).
  Fixed: zero-area chimney / opening (MCP refusal + `bad_size` problem),
  plan sheet foot wraps instead of dropping line totals, new job ids under
  a library lock, all-or-nothing delete and restore, synced saves, library
  listing within 500 ms per job (`figures_pending`), `refused` error code
  for core refusals, dash for a roof that is not calculated, wall bar
  limits, double click opens the pressed row, undo keeps the selection.
- Open for Jakub: MCP `path` arguments reach any file (ARCH-002); a field
  added without a format-version bump is dropped by the previous release
  (ARCH-003, building height in 0.7.0); Export dialog shows the raw
  price-sheet text (UX-003); pitch over 90° only marked, not refused.
- Open, not started: ARCH-004 (edit operations copied in TypeScript),
  ARCH-005, UX-004/006/007/008/011-015/017-020, TEST-002..008, UI-006/007/009.
  The calculation still has to leave out a zero-area chimney from an old
  file (`cleaned()` in openings.rs, core thread). ERR-001 (ordinary edits
  ending in a roof error) is with the core thread.

## 2026-11-15 - ARCH-004: the editor calls the core (branch claude/project-thread-14l1th)
- Moved to core commands (`commands.rs`, Tauri, `ui_bridge.rs`, `api.ts`): calculation helpers (`roof_helper_context/preview/apply`, `edit::helper`, also behind MCP `roof_helper`), round-roof arc (`round_arc`), opening reshape / chimney from rectangle / face area / regular polygon / part centre / merge of a dropped corner / drawn footprint without repeats (`item_geometry`), part transform and wall shift (`transform_part`, `offset_wall`). New `edit/helper.rs`, `edit/corners.rs`; `lib/latest.ts` (debounced, newest answer wins) for live figures.
- Deleted TypeScript: `roofcalc.ts` (+ test), `oppositeWall`, `spanBetween`, `reshapeOpening`, `chimneyFromRect`, `faceArea`, `offsetWall`, `regularPolygon`, `partCentre`, `mergeDroppedCorner`, `withoutRepeats`; 21 `editparity` ops left with their copies (fixture and `edit_parity.rs` unchanged).
- Still in TypeScript, on purpose (drag-time presentation) or not yet moved: see ADR-0005 (Proposed). Not yet moved: `dormers.ts` dialog logic with `roundroof.ts`'s arc (DormerBar/DormerDialog read it synchronously in `$derived`; needs both components restructured around async answers, see ADR-0005) and `buildingHeight` / `freezeHeight` in the wall bar's edits (needs a serial edit queue).

## 2026-11-15 - the rest of the 2026-11-14 review findings (branch claude/project-thread-14l1th, PR #48)
- Done: an AI client reads a file outside the jobs folder and the drawings
  inbox only after the user allowed that file in the app (`file_request`,
  `allowed_files`; ARCH-002, Jakub's choice). Format version 3 with kept
  unknown top-level keys and a schema-hash guard test (ARCH-003, ADR-0004
  Proposed). One calculation per request (ARCH-006). svg / xls / doc shown
  in their folder (ARCH-005). Editor: eave line, typed angle, lengths while
  drawing, labels of several buildings, keyboard stepping, placing bar.
  Results and export: translated table of the copied list, line names,
  pitch over 90° refused at input, AI review shows the change beside each
  wall. Tests: E2E run slots, MCP parity requires a change, codes tied to
  UI texts, round roof closed forms.
- State: `make check` exit 0, `make sweeps` pass, Playwright 296 passed.
- Open for Jakub: accept ADR-0004 and ADR-0005; remove or rewrite the old
  ignored property `face_areas_are_not_negative` (replaced by
  `a_negative_face_area_always_carries_cut_exceeds_face`); whether the
  round-roof reference runs become a committed fixture (now read through
  `DACAD_ROUND`). Not done: `dormers.ts` still copies `edit::dormers`
  (ADR-0005 inventory); unknown keys below the top level are not kept;
  Tauri-only behaviour (watcher, quit guard, updater) has no automated test.

## 2026-11-15 - several low wings, cm-turned buildings and the audit's core findings (branch claude/project-thread-lnaes1, PR #50)
- Done: bounded multi-wing peel (`wing_split.rs`, up to 4 wings) with an
  explained-overlap branch in `check::sound` (Jakub's decision); `straighten.rs`
  snaps cm-rounded turned outlines by at most 1 cm (Jakub's decision); the
  departure chain runs in the building's local frame within one budget of
  64 core runs (`budget.rs`); `DormerOffRoof` on departed roofs; stale ridge
  index panic in `drop_short_junction_hips`. Audit 2026-11-15 core findings:
  six new warnings (`doubled_line`, `far_from_origin`, `no_roof_area`,
  `objects_overlap`, `dormer_above_roof`, `inside_another`), checked fallback,
  chimney without height, material 0 in totals, dormer deduction, ridge under
  a chimney with a joined wing, sweep level cap, neutral no-end text.
- State: `make check` exit 0, `make sweeps` pass; 209-job fuzz corpus and all
  105 DACAD fixture projects unchanged. Reviews: target/review/core-leftovers-r1,
  -r2, audit-core-r1 (copies in /mnt/project-files/core-leftovers-2026-11-15/).
- Open for Jakub: run the standard core in the building's local frame
  (origin independence); MATH-002 cross-building overlap warning (patch in the
  project files, changes the expectation of `joined_buildings_are_sound`);
  DACAD runs for the ridge-height pitch break, `two_touch_high`, a row of two
  equal houses (verge at the shared wall), material 0 in the list, and the
  flush L-shaped low arm.

## 2026-11-15 - pre-release audit findings in the editor, MCP tools and export (branch claude/project-thread-14l1th)
- Done (audit board /mnt/project-files/audit-2026-11-15/, IDs are the
  audit's own): ERR-001, HUNTC-003 (MCP writes only), MATH-005/006,
  MATH2-004, HUNTC-001 (MCP only), HUNTC-005, ERR-007, ARCH-001, ARCH-005,
  HUNTIO-003, ERRU-001/002/003/005/006/008/009/010, UI-001, UI-010,
  UX-001..012. A typed wall length goes through the core
  (`edit::corners::set_wall_length`) and keeps right angles; the library
  has a trash with restore; an offer job lists its stored area.
- State: `make check` exit 0, `make sweeps` pass, Playwright 319 passed.
- Open: ARCH-009 (PNG over MCP is refused with a reason; a real PNG needs a
  rasteriser dependency such as resvg, owner's decision); the editor's own
  save does not refuse a window larger than its face (it would break
  autosave); core `transform_part` still stacks a mirror with copies > 1
  (parity fixture case 3102 encodes it, changing it is a human decision);
  change-log stamps are not unique per job (store.rs, audit thread).

## 2026-11-15 - pre-release audit 0.9.0 and its core/io fixes (branch claude/project-thread-82j058)
- Done: whole-app audit with the six reviewers, two bug hunters and
  verifiers (run 2026-11-15-9d6c06c; summary and boards in the project files,
  `audit-2026-11-15/`). Fixed here: half-hip pitch under 2° or below the
  eave is a problem, not a core panic (ERR-010, HUNTC-009); crossing walls
  of one building are a problem (HUNTC-002); roof defaults stay in the
  range a new wall accepts (HUNTIO-007); MCP answers a non-UTF-8 line and
  batches (ERR-002/003); a second AI revert goes one more step back, a
  failed save logs nothing, log times increase (HUNTIO-002/005, ERR-008,
  ERRU-009); bundles export only what import accepts, atomically, and bring
  no unchecked history or program files (ARCH-003, HUNTIO-004/006/014); a
  removed file returns to its own job and a vanished jobs folder is not
  re-created (HUNTIO-010/011); folder link plus `..` and UNC paths no longer
  slip past the AI file check (ARCH-002/006).
- State: `make check` exit 0 (500 Rust tests, 226 vitest, UI build).
- Open: the other S2/S3 findings in the audit summary's fix order (MATH-001,
  MATH-002, MATHC-001, HUNTC-003/007, MATH2-005, ERR-001, ERRU-005/010,
  TEST-003/009 and the UI/UX S3 list); DormerBar still lets a half-hip
  pitch go down to 0.1°, which is now reported as a problem.

## 2026-11-21 - sheet metal colour, tiles area check, roof line details (branch claude/project-thread-46ju0l)
- Done: faces of a material other than tiles have their own colour token
  (`--mat-sheet`, `--mat-stone`, `--mat-slate`, `--mat-solar`) in 3D, as a
  tint in the plan and as a swatch in the Results material table. Checked
  that sheet metal is not in the tiles' area (results `area_by_material`
  and the price-sheet list rows): test `sheet_metal_is_not_counted_as_tiles`;
  DACAD also keeps one area group per material. New core `lineinfo`
  (each line's length, plan length, slope, end heights, bounded faces) in
  the UI calc result and as MCP tool `line_details`; hover or click a roof
  line in plan or 3D shows the `LineInfo` card and draws the line amber
  (`--line-hl`, `--line-hl-edge`).
- State: `make check` exit 0, i18n sweep clean, contrast pass.
- Material check (Jakub: "really important"): `tests/material_split.rs`
  puts every wall of every example variant (dormer, chimney + window,
  clad gables + breaks, two and joined buildings) in sheet metal in turn:
  only its own faces change material, the roof is unchanged and the split
  adds up; list rows equal the results. Found: two walls in line with the
  same eave height and overhang but different materials are joined into
  one plane (DACAD's collinear merge ignores material), so the second
  material vanished silently. Jakub chose to split such a plane per wall:
  `material_split.rs` cuts its faces square to the wall line where the
  material changes (strip per wall, deductions placed per strip, lines
  crossing a cut split); `merged_materials` is only the fallback where the
  split cannot be exact. Closed-form tests (hip strip 6.125/cos 35°, gable
  middle strip 4 × 4.5/cos 40°, windows + chimney across the cut). Lean-to
  overhang at the house wall checked: not counted when the roofs join; a
  house without overhang and a steep lean-to do not join and count it
  (told Jakub, PR #54 thread owns overhang rules). Hand-drawn faces always
  count as material 1.
- Open: line details have no keyboard path in the plan (the Calculation tab
  and MCP cover the figures); a pinned card closes with Esc, × or a second
  click on the line, not with a click on empty plan.

## 2026-11-21 - verification of every roof calculation (branch claude/project-thread-5ugj01)
- Done: DACAD parity rerun (dcore byte-exact on 1 967 DACAD SO2/INT
  captures from the project files; samples 29/34 as before), and an
  independent check of 3 619 roofs with the new `verify_calcs` example
  (face area vs polygon, pitch vs plane, chimney deduction vs plan cut,
  plan coverage, results = breakdown = price-sheet list = stored result).
  Fixed: joined faces on swept roofs dropped their window/chimney
  deductions (list took a window off twice); wing windows' face index.
  Guard: `tests/calc_consistency.rs`.
- State: `make check` exit 0 (618 Rust tests, 263 vitest, UI build).
- Open (DACAD behaviour, reported to Jakub, not changed): face areas of
  skewed hip faces up to ~0.7 % under their polygon; chimney deduction
  across a hip up to ~0.05 m2 off the plan cut; joined-roof pieces
  (Modellverschnitt) book areas by DACAD's rules, up to several % per
  piece; a hipped bay whose front plane lies ~2 cm under the house plane
  (Jakub's test job) counts ~0.3 m2 twice in slivers; a cut larger than
  its face on joined buildings still gives a negative face (warned).
- Rounds 2-4 (Fable review + bug hunt each round, fixes with failing-first
  tests): stored-result refresh (`Store::refresh_stored_result`, sidecar
  `.activity/<id>.result.json` with up to 16 origins by `taken_ms`; origin
  only when this engine gives the figures for the design, at most 200
  walls+breaks+objects; no refresh while an AI review is pending; an
  engine entry ends a pending review); new warnings
  `hidden_under_neighbour`, `uncovered_plan`; join keeps chimney/dormer
  lines and books line deductions by place (`RoofGeometry::minus_at`);
  `cut_chimney` shares a hole edge only along the other edge; departed
  roofs keep windows larger than a face. Guards in `calc_consistency.rs`,
  `leftovers_cases.rs`, `store.rs` tests.
- State: `make check` exit 0 at 481fe34 (648 Rust tests, 264 vitest).
- Open: the refresh only acts after a version bump (results stored by
  0.9.0 carry 0.9.0); results stored before the sidecar existed are never
  refreshed; a roof opening side exactly on a ridge/seam of a swept roof
  loses that side (same on main); chimney deduction on some core roofs
  changes with the building's position (Fable R3-2, needs a DACAD run);
  opening inside a dormer on swept roofs deducted from the host too (R3-5,
  same on main); chimney front side within 1 cm above a joined break and
  across the ridge not deducted (R3-4, same on main).

## 2026-11-24 - repeated whole-app audit, rounds 1-21 (branch claude/project-thread-6i618l, PR #55)
- Done: 21 audit rounds (six tectora-review reviewers + bug hunter, two
  independent verifiers per round), every verified finding fixed with a guard
  test. Boards and screenshots in the project files `audit-2026-11-21/`.
  Round 7: plan drag as one transaction (`dragsession.ts`). Rounds 15-21: the
  AI review moved from a log walk to the job's version (ADR 0006) and then to
  an explicit baseline file `.activity/<id>.baseline.json` (ADR 0007, with
  round-21 amendments R10 outside saves, older Tectora's Accept, R7 swap).
  Round 21 also: library mark and filter "waits for review" (MCP
  `list_projects` `review_pending`), banner = net change against the approved
  state, overlapping hand-drawn faces warn, place_item inside the work bound,
  no-corner Ctrl+Z / Backspace / Delete do nothing. Version 0.10.0.
- State: make check exit 0, make sweeps exit 0, tectora-app clippy (fake
  pkg-config) exit 0, make e2e 714 passed at the round-21 merge.
- Next: Jakub accepts ADR 0006/0007 and merges PR #55, then tags v0.10.0
  (notes in the project files `releases/release-0.10.0-notes.txt`).
- Blockers / open questions: round 21 was declared the last review round by
  Jakub; its known-open list (TEST-2012 has no deterministic E2E,
  `folded_loop` not under the work bound, `merge::value_at` and keys with
  `.` or `[`) is in the round-21 run folder `known-open.md`.

## 2026-11-28 - object editor redesign (branch claude/project-thread-1c8ssz)
- Done: the chimney / roof window / roof opening editor (docked bar and its
  Properties dialog) shows three named groups: size, position on the roof
  (from eave / left / right), position in plan (corner X / Y, rotation; in
  the bar under More, `prefs.objectMore`). Facts: face and pitch, height of a
  chimney top above the roof, area on the slope (= what the results deduct),
  window height in plan. The dialog has a sketch of the host face turned eave
  down (`PlaceSketch.svelte`, above a pitch break it marks the break), and
  delete moved to its footer. Presentation helpers in `objectfacts.ts` with
  unit tests; E2E `object-editor.e2e.ts`. Designed and reviewed by Fable
  (spec and review in the project files `object-editor-2026-11-28/`).
- Open: "height above the roof" and the below-roof warning are UI-only; an
  MCP client cannot read them yet (core `Placement` follow-up). The dormer
  bar's More row got the same group captions; the dormer dialog is unchanged.
