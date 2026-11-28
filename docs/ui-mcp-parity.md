# UI / MCP parity

An AI client working through the MCP server must be able to see everything
the user sees in the app and do everything the user can do there, so that
the quality of AI work is limited only by the model (Jakub, 2026-11-28;
`AGENTS.md`, "MCP parity"). The exceptions are the user's own answers to an
AI (accepting its changes, allowing a folder or a file, connecting AI apps)
and things with no information in them (focus handling, layout).

## The list

`docs/ui-mcp-parity.json` names, for every Tauri command and every Svelte
component and TypeScript module under `app/src`, either the MCP tools that
give an AI the same information and actions (`mcp`, optionally `checks`
with answer fields), or the reason there is nothing to give (`user_only`,
`no_data`). An entry may carry `open_gap` for something still missing.

`crates/tectora-mcp/src/parity_tests.rs` fails when

- a Tauri command, component or module is missing from the list, or a
  listed one no longer exists,
- a named MCP tool does not exist,
- a `checks` path (`tool:path.to.field`) is missing from that tool's answer
  for a busy test job (chimney, window, opening, dormer, hand-drawn face,
  half-hip, clad gable, stored result, pending review).

## Rules for a new feature

1. Compute what the UI shows in `tectora-core`, not in TypeScript. A value
   the editor needs while the user types may keep a TypeScript copy only
   with cases in `tests/fixtures/edit/parity.json` (`edit_parity.rs`, the
   vitest `editparity.test.ts`) that prove both give the same answer.
2. Return it from the MCP tool that reads the same thing (`get_project`,
   `calc`, `calc_breakdown`, `line_details`, `get_history`, `get_review`,
   `list_projects`, `export_list`, `roof_helper`) and describe the field
   in the tool's description.
3. Add the module or component to `docs/ui-mcp-parity.json` with a
   `checks` path for each new field.

## Where things are

| the user sees | the AI reads |
| --- | --- |
| walls: pitch, heights, overhang, half-hip, breaks, clad gable, direction change | `get_project` `parts[].edges[]` |
| field marks: unusual values, half-hip / break above the ridge | `calc` / `get_project` `input_notices` |
| object editor: distances, face, pitch, area taken off, chimney above the roof, box, window outline | `get_project` `placements` |
| dormer dialog section, dormer above the roof | `get_project` `dormers[].section` |
| results panel: areas, lines, materials, warnings, objects on and off the roof, hand-drawn face areas | `calc` |
| calculation tab | `calc_breakdown` |
| line info | `line_details` |
| calculation and round-roof dialogs | `roof_helper` (tool `context` first) |
| history with its changes | `get_history` |
| review banner and wall notes | `get_review` `pending.changes` |
| library cards: offer area, differs mark, AI tag, notes, blocking entries | `list_projects` |
| export dialog: text and tables | `export_list` `list`, `summary` |
| roof height under the cursor | `roof_at` |
| toolbar Chimney / Window / Opening | `add_object` |
