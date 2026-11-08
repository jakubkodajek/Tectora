# AI integration (MCP)

Tectora ships a [Model Context Protocol](https://modelcontextprotocol.io/specification/2025-06-18)
server so AI clients can create and edit roof designs while the user watches in
the desktop app.

## Starting the server

- Installed app: `<path to Tectora executable> mcp` (the same binary as the GUI;
  with the `mcp` argument it serves MCP on stdin/stdout and opens no window).
- Development / CLI: `tectora mcp [--dir <library folder>]`.

Both use the same project library as the app: `$TECTORA_PROJECTS_DIR` or
`Documents/Tectora`.

## Connecting an AI app

The app's "Connect AI" dialog lists the AI apps installed for this user and
connects each with one click (`tectora_core::assistants`, the approach of
Jakub's Buzzdome app). Connecting adds one entry, `tectora`, with this
executable and the `mcp` argument, to the app's own settings file and keeps
everything else in it (TOML comments too). Nothing else changes; the AI app
must be restarted (or its MCP list refreshed) to pick it up.

| AI app | Settings file | Entry |
| --- | --- | --- |
| Claude Desktop | macOS `~/Library/Application Support/Claude/claude_desktop_config.json`; Windows `%APPDATA%\Claude\`, plus the packaged (MSIX) app's own copies in `%LOCALAPPDATA%\Packages\Claude_*\LocalCache\Roaming\Claude\` and `%LOCALAPPDATA%\Claude-Data\claude_*\` | `mcpServers.tectora` |
| Codex (ChatGPT desktop app, Codex CLI and IDE extension) | `~/.codex/config.toml` or `$CODEX_HOME/config.toml` | `[mcp_servers.tectora]` |
| Antigravity | `~/.gemini/config/mcp_config.json` | `mcpServers.tectora` |

An app counts as installed when the folder it makes on first start exists.
A row says "Needs connecting again" when an entry starts another program
(Tectora moved, or an entry pasted by hand points at an old copy). A settings
file that is not valid JSON/TOML is left untouched and the error is shown.
For any other app the dialog gives the `claude mcp add` command for Claude
Code and the JSON entry to paste. ChatGPT on the web cannot start local
programs; Codex in the ChatGPT desktop app can.

## Tools

| Tool | Purpose |
| --- | --- |
| `tectora_guide` | Model conventions (DACAD terms, units) and the drawing-to-roof workflow |
| `list_projects`, `get_project`, `calc` | Search the library (text, status, AI change, dates, area, sort), read designs, edge tables, dormers, quantities by DACAD line code, stored result |
| `calc_breakdown` | How every face area and line length was calculated, step by step (the app's Calculation tab) |
| `create_standard_roof`, `create_project`, `create_from_example` | Create designs |
| `add_part`, `add_shape_part`, `transform_part`, `delete_items` | Parts: add (standard roofs, regular polygons), move / rotate / mirror / copy, delete anything |
| `update_edges`, `set_outline`, `edit_walls` | Walls and their roof definitions (incl. clad gable and the direction of a direction change); corners, lengths, direction changes, parallel shift |
| `add_dormer`, `update_dormer` | Dormers by kind (gable, pointed, hip, half-hip, shed, trapezoid, round) |
| `set_openings`, `set_manual`, `set_roof_options` | Windows, chimneys, openings; hand-drawn lines and faces; flat roof, material names |
| `transform_items` | Move, rotate, copy or align to the nearest wall single dormers, chimneys, windows, openings, hand-drawn lines and faces (the plan's drag and right-click menu) |
| `roof_helper` | Calculation helpers (triangle, two parallels, point in space, merge, hip/valley, build-up, round roof), preview or apply |
| `update_job`, `store_result` | Job data and status; the stored result for the offer |
| `get_history`, `get_review`, `revert_to`, `revert_last_change`, `show_project` | History, review state (accepted or still pending in the app, effect on the figures), undo, focus in the app |
| `duplicate_project`, `delete_project`, `list_examples` | Library; `duplicate_project` with `job` starts a new job from a past roof |
| `list_files`, `attach_file`, `remove_file` | Files attached to a job (drawings) |
| `export_job`, `import_job`, `get_schema` | `.tectorajob` bundles, file schema |
| `get_settings`, `update_settings` | App settings shared with the Settings screen: jobs folder, drawings inbox folder, language, theme, defaults for new roofs, follow mode |
| `list_drawings`, `drawing_info`, `view_drawing`, `drawing_dimensions`, `trace_outline`, `drawing_lines`, `measure_drawing` | Technical drawings (PDF in the drawings inbox or attached to a job): page size and kind, rendered pages and crops with a sheet-mm grid, every dimension chain with its drawn length, an outline snapped to the drawn corners and returned in metres, the exact vector lines and the text-layer labels (filtered, e.g. every height label), measuring between two points against a dimension the AI read, and the calculated roof drawn over the plan (`view_drawing` `overlay`) to check it |

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
  can be turned off) and shows a banner with every pending step's summary, the
  list of changes and how they change the figures (roof area, ridge height,
  line lengths by code, area by material: before → now, with the difference).
  Changed walls, dormers, chimneys, windows, openings and hand-drawn lines and
  faces are highlighted in orange; the previous outline of moved walls and the
  old place of moved or removed items are drawn dashed ("Previous state"
  toggle). Hovering a change in the AI tab shows only that change.
- The changes are computed per item (`tectora_core::diff`): one entry per
  dormer, chimney, window, opening, hand-drawn line or face (`item` index),
  plus flat roof, material names, wall material and the stored result.
- Accept / Revert all act on every pending step at once. The review state is
  kept in the library (`.activity/<id>.review.json`), so it survives a restart
  and the agent can read it with `get_review`; only the user accepts.
  The AI tab keeps the full history.

A skill for Claude that describes the drawing workflow is in
`integrations/skills/tectora-roof/SKILL.md`.
