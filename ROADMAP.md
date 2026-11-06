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
- Direct editing in the plan and the 3D view: drag dormers, chimneys,
  windows, openings, hand-drawn shapes and the selected part; right-click
  menus (properties, duplicate, rotate, align with wall, delete, eave/gable,
  insert point, add an object here); arrow keys nudge, Ctrl+D duplicates.
- 3D view shows chimneys (with cap), roof windows (frame and glass), roof
  openings cut into their face and dormer cheeks and fronts.
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
  and putting the walls and roof definitions together correctly. Needs a
  test set of real drawings with the roofs DACAD gave for them, to measure
  how often the AI gets the roof right.
- Review flow: the AI marks what it was unsure about (unreadable dimension,
  guessed pitch); the colleague confirms or fixes it before the job leaves
  draft.
