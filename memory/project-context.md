# Project context

## Product

- **One-liner:** a desktop roof design app that replaces DACAD for TERRAN, a
  roofing company: draw the walls, define the roof per wall, get the roof,
  its area and its line lengths for the offer.
- **Problem:** DACAD is old Windows-only software; TERRAN needs the same
  results in a modern app on macOS and Windows, with a job library and with
  AI building roofs from technical drawings.
- **Users:** TERRAN colleagues who prepare offers (roofers, not CAD experts),
  and AI clients (Claude Desktop, Codex, Antigravity) driving Tectora over MCP.
- **Owner:** Jakub Kodajek. Only Jakub accepts ADRs and changes DACAD fixtures.

## Constraints

- **Results:** identical to DACAD, byte-exact where DACAD writes files.
  DACAD is the reference, but its name and material never appear in the
  product or the repository (`AGENTS.md`, Rules).
- **Platforms:** macOS (universal .dmg) and Windows (.msi/.exe), both
  first-class. Linux only for development and CI.
- **Offline first:** the app works without a network. Today Tectora calls no
  LLM API; AI clients drive it from outside over MCP (an in-app AI option is
  only an idea in `ROADMAP.md`, section "AI").
- **Data:** jobs are plain `.tectora` JSON files in a folder
  (`Documents/Tectora` or `$TECTORA_PROJECTS_DIR`), readable by the app,
  the CLI and the MCP server at the same time.
- **Languages:** Czech (default), Slovak, English, German.

## Glossary (use these terms consistently)

- **job / zakázka:** one saved roof design with customer, address, status
  (draft, offer, ordered, done), files and change log.
- **part:** one closed wall loop with its own roof; **dormer / vikýř**,
  **chimney / komín**, **roof window / střešní okno**, **opening / otvor**.
- **eave (T) / gable (G):** the two roof definitions of a wall; DACAD line
  codes (T, F, G, K, O, WV, WS, …) label roof lines in results.
- **pitch break (Knick), half-hip (Krüppelwalm), overhang, eave height,
  priority:** per-wall roof parameters.
- **dcore:** the port of DACAD's roof core; **Modellverschnitt (modell):**
  DACAD's stage that joins several roofs; **RD2 / SOL / INT / W3D:** DACAD's
  intermediate files the port reads and writes.
- **stored result:** the figures an offer was made with, kept in the job.
- **MCP parity:** everything the editor can do, an MCP tool can do.
