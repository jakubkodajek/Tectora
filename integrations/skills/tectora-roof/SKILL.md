---
name: tectora-roof
description: Turn a technical drawing (plan, elevations, sections) into a roof design in the Tectora app via its MCP server, so the user can review it live. Use when the user shares a building drawing and wants the roof in Tectora.
---

# Tectora roof from a drawing

Tectora is a DACAD-style roof design app. Its MCP server (`tectora` tools) writes
designs into the user's library; the open Tectora window hot-reloads every save,
highlights what changed and shows your `summary`, with Accept / Revert buttons.

1. Call `tectora_guide` once and follow its conventions (metres, x right / y up,
   wall-line footprint, one roof definition per wall: eave or gable, pitch,
   eave height, overhang, half-hip).
2. Read the drawing. Note scale, footprint dimensions, which walls are eaves and
   which are gables (elevations/sections), pitches, eave height and overhang.
   List anything you cannot read and ask instead of guessing. If the user did
   not attach the drawing, `get_settings` gives `effective.drawings_dir`, the
   folder where they put drawings; its `roof_defaults` are the user's usual
   pitch, eave height and overhang (say when you used one).
3. Search first with `list_projects` (customer, address, job number) so you edit
   the existing job instead of creating a duplicate.
4. Create: `create_standard_roof` for a plain rectangle, otherwise
   `create_project` (one part per wing with its own ridge height). Fill `job`
   (number, customer, address) when the drawing's title block has them.
5. Check with `calc`: roof area, ridge height and line lengths (T, F, G, K, O, W)
   must be plausible against the drawing. When the user asks how a number came
   about, `calc_breakdown` gives every face (plan area / cos pitch − openings)
   and every line with its 3D length.
6. Correct with `update_edges`, `set_outline`, `add_part`, `update_job`. Make a
   few meaningful steps, each with a one-sentence `summary` in the user's
   language (it is shown to them), e.g. "Sklon uliční strany 42° podle řezu A-A".
7. Finish by telling the user what you assumed and asking them to review the
   highlighted changes in Tectora.
