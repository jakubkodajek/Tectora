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
  areas within its rounding, job 9 within 1.3 %, object A of job 4 within
  2.3 %. Job 6 (30 dormers) matches the exact Bramac lines (valleys 0.1 %,
  verges 0.8 %, eaves 2.4 %) and its gross faces; its area is 4.7 % under
  because Bramac deducts less under each dormer. Open with the estimator:
  job 8 (offer 23 % below the roof its ridge and eaves describe), object B
  of job 4 (+7.3 %), jobs 1 and 2 (no section; contradictory labels on 1).
  **Next:** the colleague's review of these builds in the app, and the
  missing sections for jobs 1 and 2.
- Review flow: the AI marks what it was unsure about (unreadable dimension,
  guessed pitch); the colleague confirms or fixes it before the job leaves
  draft.
