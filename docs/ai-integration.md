# AI integration (MCP)

Tectora ships a [Model Context Protocol](https://modelcontextprotocol.io/specification/2025-06-18)
server so AI clients can create and edit roof designs while the user watches in
the desktop app.

## Starting the server

- Installed app: `<path to Tectora executable> mcp` (the same binary as the GUI;
  with the `mcp` argument it serves MCP on stdin/stdout and opens no window).
- Development / CLI: `tectora mcp [--dir <library folder>]`.

Both use the same project library as the app: `$TECTORA_PROJECTS_DIR` or
`Documents/Tectora`. The app's "Connect AI" dialog shows the exact command and
ready-to-paste configuration for Claude Desktop, Claude Code and Gemini CLI.

## Tools

| Tool | Purpose |
| --- | --- |
| `tectora_guide` | Model conventions (DACAD terms, units) and the drawing-to-roof workflow |
| `list_projects`, `get_project`, `calc` | Read designs, edge tables, dormers, quantities by DACAD line code, stored result |
| `create_standard_roof`, `create_project`, `create_from_example` | Create designs |
| `add_part`, `add_shape_part`, `transform_part`, `delete_items` | Parts: add (standard roofs, regular polygons), move / rotate / mirror / copy, delete anything |
| `update_edges`, `set_outline`, `edit_walls` | Walls and their roof definitions; corners, lengths, direction changes, parallel shift |
| `add_dormer`, `update_dormer` | Dormers by kind (gable, pointed, hip, half-hip, shed, trapezoid, round) |
| `set_openings`, `set_manual`, `set_roof_options` | Windows, chimneys, openings; hand-drawn lines and faces; flat roof, material names |
| `roof_helper` | Calculation helpers (triangle, two parallels, point in space, merge, hip/valley, build-up, round roof), preview or apply |
| `update_job`, `store_result` | Job data and status; the stored result for the offer |
| `get_history`, `revert_to`, `revert_last_change`, `show_project` | History, undo, focus in the app |
| `duplicate_project`, `delete_project`, `list_examples` | Library |
| `list_files`, `attach_file`, `remove_file` | Files attached to a job (drawings) |
| `export_job`, `import_job`, `get_schema` | `.tectorajob` bundles, file schema |
| `get_settings`, `update_settings` | App settings shared with the Settings screen: jobs folder, drawings inbox folder, language, theme, defaults for new roofs, follow mode |
| `list_drawings`, `drawing_info`, `view_drawing`, `drawing_lines`, `measure_drawing` | Technical drawings (PDF in the drawings inbox or attached to a job): page size and kind, rendered pages and crops as images, the exact vector lines, and measuring between two sheet points snapped to line ends, checked against a dimension the AI read (`expected_mm` + scale) |

Every write requires a `summary` sentence for the user. The drawing tools only
read; they are implemented in `crates/tectora-drawing` (pure Rust, `hayro`
PDF renderer).

## What the user sees

- Every write goes through `Store::save_as`, which appends an entry to
  `.activity/<id>.jsonl` with the actor (the MCP client name), the summary, the
  computed changes and the previous version.
- The server also writes `.activity/live.json` on each call ("Reading …",
  "Calculating …", or the write summary).
- The app watches the folder: it opens the job the agent works on (follow mode,
  can be turned off), shows a banner with the summary and the list of changes,
  highlights the changed walls in orange, and offers Accept or Revert.
  The AI tab keeps the full history.

A skill for Claude that describes the drawing workflow is in
`integrations/skills/tectora-roof/SKILL.md`.
