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
   inbox). `drawing_info` says per page whether it has a text layer, is a
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
   in the drawings go to the user as questions, not guesses.
5. Search with `list_projects` (customer, address, job number) so you edit the
   existing job instead of creating a duplicate.
6. Build in the six steps of the guide: read and check, then one save each for
   footprint (with job data, then attach the drawings), roof definition,
   dormers, chimneys and windows, and the check report in the job notes.
   Each `summary` is one sentence in the user's language
   naming the sheet, e.g. "Sklon uliční strany 42° podle řezu A-A".
7. In the check step compare `calc` with every figure the drawing prints
   (area per pitch, plan area, ridge and eave heights) and finish by telling
   the user what matched, what you assumed and what they should look at in
   the highlighted changes in Tectora.
