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
   height, overhang); say when you used one. `drawing_info` says per page whether it has a text layer, is a
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
   A wing whose eave is lower or higher than the main roof's gets its
   height with `set_part_height` (all its walls in one save); the roofs
   are joined by those eave heights.
   Each `summary` is one sentence in the user's language
   naming the sheet, e.g. "Sklon uliční strany 42° podle řezu A-A".
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
   lost more than it has; never bend the input to make a warning go away.
8. When the roof is approved and the user wants it for the offer,
   `export_plan` writes the roof sheet (PDF) and `export_list` returns the
   text for the Excel price sheet; give the text verbatim.
