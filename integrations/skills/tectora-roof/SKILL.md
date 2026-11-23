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
   eave height, overhang, half-hip) and its six-step order.
2. Find the drawing: attached by the user, or `list_drawings` (the drawings
   inbox). `get_settings` gives the user's usual `roof_defaults` (pitch, eave
   height, overhang); say when you used one. Do not change them with
   `update_settings` for one roof: the user sees every settings change and
   may revert it. `drawing_info` says per page whether it has a text layer, is a
   vector plot with outlined text, or a scan.
3. Read it with `view_drawing`: the whole page first (turn sideways sheets
   with `rotate` until the text is upright), then sharp crops (about 300 dpi)
   of the dimensions, pitch and height labels, title block. Do not read
   numbers from a whole-sheet image. Read coordinates off the orange sheet-mm
   grid on every image.
4. Get lengths from the drawing, not from pixels: `drawing_dimensions` gives
   every dimension's drawn length, `trace_outline` the exact outline in
   metres. First work out the plot scale from two or three long dimensions
   (written value / `sheet_mm`): sheets are often printed at another scale
   than the title block says. Say which written values disagree with the
   drawing; on scans say which values are only read, not measured.
   The guide's "Reading the drawing" section lists what tripped earlier
   builds: the gutter strip is not overhang, nearly straight corners break
   the calculation, metal decks in a tiled slope are shed dormers, and gaps
   in the drawings go to the user as questions, not guesses. Its "Roof
   form from the elevations" and "Pitch breaks" paragraphs say how to read
   wings whose ridges are level and eaves with a flatter strip; "Heights"
   says to check every eave label against the ridge and pitch, and
   "Half-hips from the plan" and "Level valleys and saddles" cover a
   half-hip drawn only on the plan and a saddle between two wings. "A range
   between hipped pavilions", "Things drawn over the roof" and "Windows
   and boxes at the eave" cover a range whose ridge is higher than its
   pavilions (one outline), walkway canopies drawn over the tiles, window
   sizes from the window schedule and attics at the eave. On a sheet with
   a text layer, `drawing_lines` with `limit: 0` and `text_contains`
   (`["+", "°"]`) lists every height and pitch label at once.
5. Search with `list_projects` (customer, address, job number) so you edit the
   existing job instead of creating a duplicate. A similar roof made before
   can be the starting point: `duplicate_project` with the new `job` data and
   `with_files: false`. Jobs listed under `broken` cannot be read (damaged or
   from a newer Tectora): tell the user instead of starting a new one.
6. Build in the six steps of the guide: read and check, then one save each for
   footprint (with job data, then attach the drawings), roof definition,
   dormers, chimneys and windows, and the check report in the job notes.
   A wing whose eave is lower or higher than the main roof's gets it with
   `set_part_height` and `eave_height` (all its walls in one save); the
   roofs are joined by those eave heights. The building height (`height`,
   top of the walls, from the sections) is a separate value: the eave at
   the overhang edge is usually lower.
   Where a drawing dimensions a dormer, chimney or window from the eave and
   from a roof edge, place it with `place_item` (`from_eave` along the
   slope, `from_left` or `from_right`) and check it against `placements`
   in `get_project`.
   Each `summary` is one sentence (at most 500 characters, plain text) in
   the user's language naming the sheet, e.g. "Sklon uliční strany 42° podle řezu A-A".
   A refused call saved nothing: a value of the wrong type or outside the
   tool's `enum` (send a job number as text), a standard roof side under
   0.1 m, a dormer below the roof or flatter than 2°, a `roof_helper`
   result no wall takes. Fix the value (often m against cm or mm) and call
   again. When a write answers with `stored_result`, the job's offer
   figures no longer match the roof: tell the user. When the user wants
   the figures the customer was offered back after Tectora recalculated
   an offer's stored result (the entry by `tectora` that `get_history`
   marks `stored_result_recalculation`, `get_project` gives it as
   `result_recalculation`), call `restore_offer_result` with a
   one-sentence `summary`: it changes the stored result only. When
   `export_job` answers with a `note`, the bundle carries the recalculated
   figures and the offer's own stay on this computer: tell the user, and
   put them back first only if the user wants to send those. A revert
   or put-back answered with `busy` did nothing (another program was
   saving the job): try it again in a moment. When
   `get_project` or a write answers with `load_warnings`
   (`result_dropped`: a stored result with an impossible date was left
   out), tell the user. When it answers with
   `warnings` (the roof looks wrong) or `roof_trouble` (for example
   `walls_overlap`: a point list that goes round twice), fix it first.
   A job `calc` refuses as "too complex to calculate" (buildings stacked
   on each other): `get_project` still lists its parts with
   `too_complex: true`; remove the copies with `delete_items`
   (`kind: part`) or `revert_last_change` your own last save, then `calc`
   again.
7. In the check step first look at the roof over the drawing:
   `view_drawing` with `overlay` {`id`, `scale`, `origin`} (the
   `origin_sheet_mm` from `trace_outline`) draws the calculated faces and
   lines over the roof plan; every drawn ridge, hip, valley and dormer
   should lie under its model line. Then compare `calc` with every figure
   the drawing prints (area per pitch, plan area, ridge and eave heights
   from `heights_m`, and with several materials the offer's material in
   `lines_by_material_m`) and finish by telling the user what matched,
   what you assumed and what they should look at in the highlighted
   changes in Tectora. When the user asks how a number came about,
   `calc_breakdown` gives every face (plan area / cos pitch − openings) and
   every line with its 3D length. Tell the user about every part's
   `warnings` and `error` from `calc` (the guide's "Warnings and errors"):
   a `covered` part has no roof of its own (move or delete it), a
   `dormer_off_roof` dormer and a `window_off_roof`, `chimney_off_roof` or
   `opening_off_roof` object are not counted, `cut_exceeds_face` means a face
   lost more than it has; `objects_overlap`, `dormer_above_roof`,
   `inside_another` and `no_roof_area` name an object or building entered
   twice, too large or too high (fix that object; an `inside_another`
   building that marks part of a face for another material stays and gets
   the pitch and eave height of the roof around it); `doubled_line` means
   line lengths at a join of two buildings need a look; `far_from_origin`
   means the plan should be moved to the origin; never bend the input to
   make a warning go away.
   `merged_planes`, `wing_joined` and `not_calculated_by_core` mean the
   roof is not the standard calculation's result (two planes joined, a low
   wing as a lean-to, the fallback): say so and have the user check it;
   `plane_over_wall` is a hint that a wing has no ridge of its own.
8. When the roof is approved and the user wants it for the offer,
   `export_plan` writes the roof sheet (PDF; pass `line_names` in the
   user's language for the legend of line codes; PDF or SVG, no PNG: the app's
   Export dialog has that) and `export_list` returns
   the text for the Excel price sheet; give the text verbatim.
