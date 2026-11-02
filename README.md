# Tectora

A modern replacement for DACAD, the roof design software: draw the walls of a
building, define the roof on each wall as in DACAD and get the roof, its area
and its line lengths. The roof calculation is a 1:1 port of DACAD's own core,
checked against the files DACAD itself writes.

Desktop app for macOS and Windows (Tauri 2 + Svelte), UI in Czech, Slovak,
English and German.

## Layout

- `crates/tectora-core` — roof project model (`.tectora` JSON), DACAD's roof
  core (`dcore`) and multi-roof join (`modell`), dormers, chimneys, roof
  windows and openings, quantities by DACAD line code and by material, job
  library with change log and attached files, job bundles (`.tectorajob`),
  stored results, editor operations (`edit`) shared with the MCP server.
  The project file format is described by
  [docs/tectora.schema.json](docs/tectora.schema.json).
- `crates/tectora-mcp` — MCP server for AI clients (Claude, Gemini, …); see
  [docs/ai-integration.md](docs/ai-integration.md).
- `crates/tectora-cli` — the `tectora` binary (`info`, `calc`, `mcp`, `schema`, `export-job`, `import-job`).
- `app/` — the desktop app: job library, DACAD-style editor (plan + 3D and
  front / back / side / top views, roof definition bar, standard roofs and
  regular polygons, dormers, chimneys, roof windows, openings, calculation
  helpers, round roofs, flat roofs, hand-drawn roof lines and faces, roof
  materials, move / rotate / mirror / copy of parts) and live review of AI
  changes.
- `tests/fixtures` — DACAD's own results the core is checked against.

## Build and test

```bash
cargo test --workspace
cargo run -p tectora-cli -- calc path/to/design.tectora --summary
cd app && npm ci && npm test && npx tauri dev
```

DACAD example files are third-party data and are not committed to this repository.
