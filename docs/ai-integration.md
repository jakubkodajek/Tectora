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
| `list_projects`, `get_project`, `calc` | Read designs, edge tables and quantities by DACAD line code |
| `create_standard_roof`, `create_project`, `add_part` | Create designs |
| `update_edges`, `set_outline`, `update_job` | Edit designs |
| `revert_last_change`, `show_project` | Undo, focus in the app |

Every write requires a `summary` sentence for the user.

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
