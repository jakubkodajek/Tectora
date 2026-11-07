# Tectora — Roadmap

The goal is every roof option DACAD offers, with DACAD's own results.
The list below is DACAD's menu (DACAD.exe string table) against the app.

## Done

- Walls, rectangle and regular polygon entry, standard roofs (gable, hip,
  half-hip, shed, pyramid), typed point coordinates and wall lengths.
- Roof definition per wall: eave or gable, pitch, eave height, overhang,
  Krüppelwalm, pitch breaks (Knick), priority, clad gable, material,
  direction markers (Null-Längen-Linie, Richtungswechsel).
- Dormers (gable, pointed, hip, half-hip, shed, trapezoid, round), chimneys,
  roof windows, roof openings.
- Calculation helpers: pitch / eave height / overhang / wall position by
  simple triangle, two parallels, point in space, merge faces; hip/valley
  pitches; build-up thickness; round roof (Runddach).
- Flat roof, hand-drawn roof lines and new roof faces.
- Move, rotate, mirror and copy parts; shift a wall in parallel.
- Front, back, side and top views; area by material; material names.
- MCP server with a tool for every editor operation, and the CLI.
- Job files: attached drawings and photos, `.tectorajob` bundles (project,
  files, history, plan preview) for hand-over and archive.
- Stored result: the figures an offer was made with, kept with the engine
  version; the app and the MCP show any later difference.
- File format: JSON Schema (`docs/tectora.schema.json`), format versions with
  migrations on load; newer files are refused.
- `.tectora` and `.tectorajob` open by double-click on macOS and Windows.

## Next

- Post-processing: trapezoid split (Trapezzerlegung), sheet layout
  (Bahnenteilung, Rollbahnenteilung), rafters (Sparreneingabe).
- Help points and lines, extra text (Hilfspunkte, Hilfslinien, Zusatztext),
  separating lines (Trennlinien), "Gaube im Grundriss", covering direction
  from wind direction (Deckrichtung).
- Variants of a job, DXF import and export, printed views and lists.

## AI: roofs from technical drawings

Goal (Jakub, 2026-11-02): an AI builds the roof from the technical drawings
by itself, and a colleague only reviews and approves it in the app. The
colleague uses Claude Desktop, Codex or Antigravity.

- An inbox folder for the drawings (PDF / images) next to the job library;
  each drawing set becomes a draft job.
- Local MCP (`tectora mcp`, already there) with hot reload into the open
  app, so every step the AI takes is visible live (follow mode, change
  highlight, accept / revert — the basis exists, see
  [docs/ai-integration.md](docs/ai-integration.md)). Check that it works
  with all three clients.
- Alternative: an AI subscription or API key connected inside the app, so no
  external client is needed.
- The hard part: reading dimensions, pitches and heights from the drawings
  and putting the walls and roof definitions together correctly.
  **Done (2026-11-06):** drawing tools in the MCP server (render pages and
  crops with a sheet-mm grid, every dimension chain with its drawn length,
  outlines snapped to the drawing, measure and check a read dimension), a
  fixed six-step build order in the guide (one save per step, so
  the colleague reviews step by step), `area_by_pitch_m2` in the quantities
  for the final cross-check, and an evaluation set of 10 real jobs scored
  against TERRAN's offers (`eval/drawings`, `tools/drawing-eval`). There is
  no DACAD roof for these jobs, so the offers are the labels.
  All ten jobs built (2026-11-06): jobs 3, 5, 7 and 10 match the offer's
  areas within its rounding, object B of job 4 within 1.0 %, job 9 within
  1.3 %, object A of job 4 within 2.3 %. Job 6 (30 dormers) matches the exact Bramac lines (valleys 0.1 %,
  verges 0.8 %, eaves 2.4 %) and its gross faces; its area is 4.7 % under
  because Bramac deducts less under each dormer. Open with the estimator:
  job 8 (offer 23 % below the roof its ridge, eaves and both sections
  describe), jobs 1 and 2 (no section; contradictory labels on 1, the 12°
  footprint of 2 is 7.6 % short whatever its form). A second pass over
  all ten jobs found three reading errors in object B of job 4 (eave
  height taken from the cornice label, a deck depth, where one deck
  starts); fixing them took it from +7.3 % to +1.0 %.
  Public roofs (2026-11-07): Czech public tenders publish the whole
  project with a bill of quantities, so they are a second set with
  known figures (`eval/drawings/public_truth.json`, numbers and source
  links only). The first two found two tool fixes (outline tracing
  pulled to the tile hatching, `drawing_lines` crowded by it) and three
  guide rules (gable edges carry no pitch breaks, level ridges mean
  steeper narrow wings built as one part per wing, bills of quantities
  are gross). A granary's bill is 14 % over its drawn roof (it counts
  over the gable walls and the eave strip twice); a villa's printed
  totals are matched by level ridges, area within 1 %, valleys 0.3 %,
  ridges and hips 1.4 %. A U-shaped hipped culture house (2134 m²) matches
  its bill's eave exactly and ridges with hips within 0.3 %, valleys
  1.1 %; the area its documents state is 9 % more with nothing on the
  sheets to explain it. It also showed outline corners read off a
  whole-page image snapping to the tile pattern, which `trace_outline`
  now reports (`edges_drawn`). Open for a DACAD parity check: that roof
  as one outline (heads at 34°, wings at 28°) fails with E00002 as soon
  as a head's ridge comes out higher than a wing's; built as parts it
  calculates. Three smaller roofs (a hipped roof at 35°, a 9° hall, a
  scanned hipped roof printed at 1:107 instead of 1:100) match every
  line their bills give within 1 %; their areas differ by +1 % (a
  rounded rafter), −0.9 % and +6 % (no formula). Four municipal roofs
  from e-zakazky.cz followed: a hipped roof at 39° and a nursery at 34°
  (all lines and the plan's stated area within 1 %), a fire station with
  a half-hip drawn only on the plan (area +0.2 % against the bill's
  formula, eaves and verges exact) and a T of hipped wings with a saddle
  in the level valley between them (ridge exactly the bill's 29.29 m;
  the bill's area is 3 % under the planes with no formula). They added
  glyph joining for CAD text layers (labels come out whole instead of
  letter by letter) and three guide rules: check every eave label
  against ridge and pitch (all `okap` labels of one plan sat 0.27 m
  above the drawn edge), half-hips from the plan, and saddles in level
  valleys as their own part.
  Job 11 (2026-11-07), a large three-storey building: two hipped pavilions
  joined by a range whose ridge is higher than theirs, a hipped wing at
  26°, a tower, sheet-metal corridor roofs and 22 roof windows. Built
  through MCP in the six steps, its tiles come out 634.7 m² against the
  offer's 630 (+0.75 %) and its ridge within 0.5 % of the bill. New guide
  rules: pavilions and range as one outline; canopies drawn over the tiles
  stand above the roof; window sizes from the window schedule; openings at
  the eave reach past it. `drawing_lines` now lists the text layer
  separately from the lines, with a filter (`text_contains`), so one call
  gives every height and pitch label of a sheet. Open for a DACAD parity
  check: two parts whose coplanar faces share an eave line lose that eave
  in the results (the range as its own part with hipped ends in the
  pavilions' outer planes).
  **Next:** more public roofs (a hipped school with mansards whose figures
  come from the designer's 3D model, an irregular faculty roof around a
  courtyard), the colleague's review of these builds in the app, and the
  missing sections for jobs 1 and 2.
- Review flow: the AI marks what it was unsure about (unreadable dimension,
  guessed pitch); the colleague confirms or fixes it before the job leaves
  draft.
