# Tectora for AI agents

Tectora is a roof design program modelled on DACAD (COIN GmbH). The user has
the Tectora desktop app open. Every change you save appears there immediately
(hot reload): the app highlights what you changed, shows your one-sentence
`summary`, and lets the user accept or revert it. Write summaries for a roofer,
in the user's language, e.g. "Pitch of the street side set to 42° per drawing".

## Model

- Units: metres and degrees. Plan coordinates: x to the right, y up (north).
- A project has one or more **parts** (roof bodies). A part is a closed
  footprint of **wall lines** (the outer face of the walls, *not* the eave
  overhang). Points are listed in order; edge `i` runs from point `i` to
  point `i+1` (the last edge closes back to point 0). The order may be
  clockwise or counter-clockwise; Tectora stores it counter-clockwise.
- Like in DACAD, **every edge carries its own roof definition**:
  - `pitch_deg` – roof pitch rising from this edge (1–89°).
  - `gable: true` – gable wall (Ortgang / štít): no roof plane rises from it
    (stored as pitch 90°).
  - `eave_height` – height of the eave line above ±0.00, m, measured at the
    outer (overhang) line as in DACAD.
  - `overhang` – horizontal eave overhang beyond the wall, m.
  - `half_hip` (gable edges only) – Krüppelwalm / polovalba:
    `{ "height": m above ±0.00 where the hip starts, "pitch_deg": … }`.
  - `priority` (0 or 1) – where this plane meets a parallel plane with a
    different pitch (e.g. a recessed or projecting wall section), the plane
    with priority 1 ends at their horizontal line; without priority the
    steeper one ends, as in DACAD.
  - `breaks` – pitch breaks (Knick / zlom, e.g. a mansard roof):
    `[{ "height": m above ±0.00, "pitch_deg": … }]`; from each height on the
    plane continues at the new pitch.
- Typical values: pitch 25–45° (tiles), 5–15° (sheet metal / shed roofs),
  eave height 3–7 m, overhang 0.3–0.8 m.
- Roof types: gable roof = eaves on the long sides + gables on the short
  sides; hip roof = eaves everywhere; shed (Pultdach) = one eave + three
  gables; pyramid = hip roof on a square; half-hip = gable roof whose gables
  have `half_hip`.

## Roof windows and chimneys

`set_openings` stores roof windows (`x`, `y` = plan point of the lower
corner at the start of the eave; `width` along the eave, `height` up the
slope) and chimneys (convex plan `corners`). Like DACAD, a window deducts
width × height and adds no lines; a chimney deducts its plan area / cos(pitch)
and adds WV (downhill), WH (uphill) and WS (sloped sides) abutment lines.

## Everything the editor can do

The tools cover every operation of the Tectora editor:

- Walls: `edit_walls` (move, insert or delete a corner, set a wall length,
  insert a direction change, shift a wall in parallel, rename the part),
  `set_outline`, `update_edges`.
- Parts: `add_part`, `add_shape_part` (standard roofs and regular polygons),
  `transform_part` (move, rotate, mirror, with copies), `delete_items`.
- Dormers: `add_dormer`, `update_dormer` (gable, pointed, hip, half_hip,
  shed, trapezoid, round); `get_project` lists them with these parameters.
- Windows, chimneys (also as a rectangle) and roof openings: `set_openings`.
- Lines and faces drawn by hand on the roof: `set_manual`.
- Flat roof and material names: `set_roof_options`.
- Calculation helpers of the roof definition: `roof_helper` (simple
  triangle, two parallel walls, point in space, merge faces, hip/valley,
  build-up thickness, round roof). It shows the values first; pass
  `apply: true` to save them.
- History: `get_history`, `revert_to`, `revert_last_change`.
- Library: `list_examples`, `create_from_example`, `duplicate_project`,
  `delete_project` (only when the user asks).
- Files of a job (the drawings): `attach_file`, `list_files`, `remove_file`.
  Attach the drawing a roof was made from, so the colleague reviewing it has
  it at hand.
- Hand-over: `export_job`, `import_job` (`.tectorajob` = project, files,
  history and a preview in one file).
- `get_schema`: the JSON Schema of the `.tectora` file.

## Stored result

When a job leaves the draft status (`update_job` with `status`), its result
is stored with the engine version: the figures the offer was made with.
`calc` and `get_project` report `stored_result` with every difference to the
current result; tell the user when there are differences. `store_result`
replaces the stored result with the current one.

## Results (DACAD line codes)

`calc` returns the roof area and line lengths keyed by DACAD codes:
T eave (Traufe/okap), F ridge (First/hřeben), G hip (Grat/nároží),
K valley (Kehle/úžlabí), OL/OR/OS verge left/right/oblique (Ortgang/štítová hrana),
P top edge of a shed roof against its wall (Pult), c pitch break (Knick/zlom),
S horizontal valley between two buildings (Schlucht),
WV/WH/WS wall abutment downhill/uphill/side (around a chimney, or where a
lower roof meets a higher building's wall).
Several parts are joined like solids: a roof is hidden inside another
part's walls below that part's roof, and roofs that cross meet in valleys.
The rules were checked against roofs calculated by DACAD itself: a step
between eaves at different heights is a short verge, the lower edge of a
Krüppelwalm plane counts as eave T, and hips run through a pitch break.

## Turning a technical drawing into a roof

The colleague reviewing your work sees every save live and accepts or
reverts it, so build the roof in **six fixed steps, one save each**, in this
order. Each step matches the sheet it comes from, so the reviewer can check
it against that sheet.

**Reading the drawing.** Most drawings are CAD plots with the text turned
into outlines: there is no text to extract, and a whole sheet scaled down to
an image makes dimension text unreadable. Use the drawing tools:

- `list_drawings` lists the PDFs in the drawings inbox; `drawing_info` tells
  each page's size and kind (`vector_with_text`, `vector_outlined_text`,
  `raster` for scans). A file can hold many pages (plans, sections,
  elevations); find the roof plan first.
- `view_drawing` renders a page or a region with an orange grid labelled in
  sheet mm (`x…`, `y…`). Look at the whole page first, then crop to about
  300 dpi to read numbers, and read every coordinate you need off the grid.
  Many sheets are plotted sideways: turn them with `rotate` until the text
  reads upright (try 90 and 270).
- `drawing_dimensions` finds every dimension chain in a region from the
  drawn lines: each dimension's ends, its middle (where its text is) and its
  exact drawn length. This is how to get dimensions, not by placing points.
- `trace_outline` snaps a polygon you give roughly (from the grid) to the
  drawn corners and returns it in metres for `create_project`, optionally
  moved in or out (`offset_m`).
- `measure_drawing` measures between two points; `drawing_lines` lists the
  drawn lines of a region and the text of PDFs that have a text layer.

**The plot scale is not always the title block's scale.** Sheets drawn at
1:50 on a large format are often printed smaller (in the test jobs: 1:50.78,
1:68.4, 1:89 instead of 1:50). Before using any length, take two or three
long dimensions, read their text, and divide by `sheet_mm` from
`drawing_dimensions`: that is the scale of this sheet. When the values
agree with each other, use that scale for everything; a single dimension
that disagrees is a misread digit or a dimension the designer overwrote
(also seen: chains whose parts do not add up to the total, so trust the
drawn geometry and say which written value disagrees).

**Which line is the roof edge.** A roof plan often draws two or three
parallel outlines: the gutter, the roof edge and the wall. The hip and
valley lines start at the roof edge; the dimensions usually measure the
outermost line the estimator priced. Use the outline the overall dimensions
measure, and check with the hips. When the walls are dimensioned (an
overhang dimension such as 500 or 900 between two lines), model walls plus
overhang; when they are not, model the roof outline with overhang 0, which
gives the same roof for planes of one pitch, and say so. A narrow strip of
about 150–160 mm outside the eave is the gutter (`žlab`, legend item
"podokapní žlab"), not overhang: the roof edge is the next line in, where
the eave height label points.

**Outline corners.** Leave out corners where the wall turns by only a
degree or two (plot noise, a wall slightly out of line): nearly straight
neighbouring eaves make the roof calculation fail. `trace_outline` warns
about them. Wings at an angle to each other are easier as separate parts,
each a clean rectangle, the second running into the first.

**Heights.** Tectora's eave height is the height of the roof plane at the
outline. Section and plan labels such as `+5,810` at a ridge are usually the
top of the covering, 5–15 cm above Tectora's ridge; use them as a check, not
as input. Read the pitch from the plan or section, never derive it from
rounded heights. A cornice label (`římsa`, `atika`) is the top of the wall,
not the eave of the roof plane, which usually sits a little higher: take
the eave height from the section's ridge and pitch (ridge height − half
span × tan(pitch)) and confirm it with a second label on the same plane,
such as the front or back of a deck or dormer.

**Roofs that are hard to model.** An eave that is lowered over part of a
wall but continues the same plane (a porch under an extended roof) is two
coplanar planes, which the roof core rejects as a single face: model the
extension as its own shed part (eave at the low height, its other three
edges gables) that touches the main eave line. Flat or sheet-metal roofs
next to the tiled roof are separate parts with their own material, or left
out if the offer does not price them; say which. Sheet-metal decks or
terraces cut into a tiled slope (a low 3–8° roof from the eave inward, with
a step up to the tiled roof behind) are wide shed dormers with material 2:
`kind: shed`, `front_pitch_deg` the deck's pitch, and the eave height chosen
so the dormer meets the roof at the drawn depth (eave height of the roof +
depth × tan(roof pitch) − depth × tan(deck pitch), roof heights measured at
the dormer front). A dormer's front must run exactly along its eave: leave
`angle_deg` out on a wall parallel to the axes, otherwise give the eave's
direction from `trace_outline` (`edge_directions_deg`); a front a fraction
of a degree off the eave makes the calculation fail. A roof that runs under
another roof's overhang up to its wall loses the part under the overhang
(the roof core hides it); other calculators may count it, so mention it
when the check against an offer is short by about that strip.

**Missing information.** When the sheets do not show something the roof
needs (no section for eave heights or the form of a low roof, labels that
contradict each other such as a pitch that does not fit the eave and ridge
heights), build with the written values, list each gap in the job notes
and ask the user; do not invent a value to make a figure fit.

**The six steps:**

1. **Read and check** (no save yet). `list_projects` first: edit the
   existing job if there is one. Note per sheet what it shows and its plot
   scale (measured as above), read the dimensions with
   `drawing_dimensions` and the outline with `trace_outline`, and work out
   the roof shape: which edges are eaves, gables, hips, what joins where.
2. **Footprint.** `create_project` with the wall-line outline, one part per
   wing with its own ridge height, in metres, and `job` (number, customer,
   address) from the title block; then `attach_file` the drawings to the
   job. Use the roof plan; the overhang outline is not the wall line (wall
   line = overhang outline minus the overhang dimension).
3. **Roof definition.** Per edge: eave or gable (elevations / sections),
   pitch, eave height, overhang, half-hips, pitch breaks (`update_edges`).
4. **Dormers** (`add_dormer`), one save for all of them.
5. **Chimneys, windows, openings** (`set_openings`).
6. **Check.** `calc`, then compare with every figure the drawing prints:
   roof area per pitch (`area_by_pitch_m2`; a drawing's "plocha střechy" is
   sloped area, "půdorysná plocha" is plan area = sloped area × cos pitch),
   ridge and eave heights (`+5,810` labels), outline dimensions. Save the
   comparison, the values you could not measure and your assumptions (e.g.
   "overhang not dimensioned, used 0.5 m") in the job's `notes` with
   `update_job`, tell the user the same, and leave the job in draft.

Skip a step that has nothing to do (no dormers) and say so. Steps 2–6 are
one save each, so the reviewer sees five changes in this order. Write every
`summary` for a roofer in the user's language and name the sheet the values
come from, e.g. "Obrys podle půdorysu střechy D.1.1.06, kóty ověřeny".

Never invent dimensions you cannot read; ask the user instead.

## Settings and the drawings folder

`get_settings` returns the user's settings, including `effective.drawings_dir`:
the inbox where the user puts technical drawings to build roofs from. Use the
`roof_defaults` (pitch, eave height, overhang) when a drawing does not give a
value, and say so in the summary. `update_settings` changes the same values as
the app's Settings screen; ask before moving the projects folder.
