# Edge cases every review must try

Bug classes the user has hit in the real app. Every reviewer reads this file,
and the reviewer named for a class runs each check in it (in the browser
preview or the E2E harness, not only by reading code) and reports a failure
as a finding. When a new class of bug reaches the user, add it here with the
test that now guards it.

The real app runs in **WebKit on macOS** and **WebView2 on Windows**; the
preview and E2E run in Chromium. Anything that depends on the web view
(native `<input type="number">`, focus on click, locale number formats,
`window.confirm`) is checked by reading the code against both, not only by
clicking in Chromium.

## 1. A typed value must reach the object it was typed for (@ui, @errors)

Guards: `app/e2e/fields.e2e.ts`, `app/src/lib/numeric.test.ts`.

For every editable field (number, text, select, checkbox) in every bar,
dialog and panel, type a new value and leave the field in each of these ways,
then check the saved file (`waitForProject`) and the recalculated result:

- Enter; Tab; Escape (must restore, not store)
- a click on empty plan; on **another wall / line / object in the plan**
  (the value must stay on the first one, the second must not change)
- a click on a toolbar or bar button (e.g. "apply to all", next wall)
- closing the editor (Back) or the window right after typing
- a decimal comma and a dot (`2,5`, `2.5`), a value out of range, text that
  is not a number (refused, field marked, reason in the tooltip, file
  unchanged)
- the ▲▼ buttons and the arrow keys (then the next arrow key must step the
  field, not move the selected object)
- a button press that keeps the focus in the field (WebKit does not focus
  a clicked button): reproduce it with `dispatchEvent("pointerdown")` +
  `dispatchEvent("click")`, a real Chromium click blurs first and proves
  nothing
- an AI / outside save arriving while the user types (the typed text stays
  and is stored with the AI change)
- a value beyond the core's limit (`1e9`, mm typed for m): every field,
  also one without its own bounds, refuses it (±100 000, `MAX_VALUE`)
- Escape inside a bar's field (e.g. the footprint wall length) only undoes
  the typing, never the bar's work (the footprint drawn so far)

No native number inputs and no hand-made ones: every number field uses
`use:numeric` (`app/src/lib/numeric.svelte.ts`); the unit test fails on a
`type="number"` input and on a text input read with `parseFloat` / `Number`
or marked `inputmode="decimal"`. A field must land on exactly its own value
(fields.e2e "every number field" diffs the file).

## 1b. Keys and double clicks that throw work away (@ux, @ui)

Guards: `app/e2e/editor-ux.e2e.ts`, `app/e2e/fields.e2e.ts`.

- Delete / Backspace with a wall selected (focus on a ◀ ▶ button) asks
  before removing a corner; a second press of a tool button that is
  already active (Building) asks before discarding corners drawn.
- A double click on every library and editor action that creates or
  deletes (Duplicate, Delete) acts once.

## 2. Every edit recalculates (@errors, @test)

After each operation that changes the job (draw, standard roof, copy,
transform, convert a manual face, undo / redo, an AI save, a merge on save)
the plan, 3D and results must show the new roof without another action.
Grep `Editor.svelte` for mutations of `project` not followed by `commit()`.

## 3. A second building (@math, @ui, @ux)

Guards: `app/e2e/joined.e2e.ts`, `crates/tectora-core/tests/check_cases.rs`
(`a_building_inside_another_is_reported_as_covered`),
`app/src/lib/scene3d.test.ts`.

Add a second building with each tool (Building, Standard roof "add",
copy, Manual face > "Make it a building") in each position relative to the
first: far apart, 0.3 m apart (overhangs overlap), touching, overlapping,
inside, identical; with the same, a lower and a higher eave height. Check:

- the new building's roof appears at once (lines in the plan, faces in 3D)
- the **first building's roof does not change shape** where the second does
  not reach it: its ridge, hips and **gables in 3D stay straight** (a gable
  cut by the other roof is concave and may start with a vertical edge)
- the plan stays readable: lines under a higher roof are dashed, the results
  name what each building is joined to, no two wall dimensions or face
  labels overlap (joined.e2e measures their screen boxes)
- the 3D view reframes so the new building is in view (`data-frame` on the
  3D host); a copy (Duplicate part) lands beside all buildings, not on one
- a building left with no visible roof face carries the `covered` warning
- the results equal the core's calculation of the saved file

## 4. Tools that look alike (@ux)

A roofer must not be able to mistake one tool for another: "Building" draws a
building with a calculated roof, "Manual face" only adds one face with a
pitch, "Flat roof" switches the whole job. Check labels and hints in all four
languages and how each result looks in the plan (a manual face is neutral and
labelled, not brick red like a building).

## 5. Core polygons drawn by the UI (@ui, @math)

Every polygon the core returns (faces, gables, clad faces, openings) must
render in plan and 3D for: a first edge of length 0 or vertical, concave
shapes, collinear corners, holes, very small and very large parts. Test the
helper that turns them into geometry with the shape the core actually
returns for the case, taken from `tectora calc`, and make sure the component
calls that helper (Roof3D draws gables through `scene3d.wallTriangles`,
which `scene3d.test.ts` covers). A roof whose core result has no gable
polygon for a gable wall (shed roof) still shows closed walls
(`wallUpToRoof`); manual faces appear in 3D; at any zoom the plan grid stays
at least 8 px apart (`grid.test.ts`).

Object bars that read a shape back from stored corners (chimney, roof
opening) must work after every way the corners can be written: placed along
a wall at any angle (corners rounded to mm), rotated, aligned to a wall,
mirrored with the part (clockwise), typed as any convex polygon over MCP.
Never fall back to "edit it in the file"; `openings.test.ts` and
`edit::openings` tests sweep all angles.

## 6. Text fields, saves and AI review (@errors, @ux)

Guards: `app/e2e/textfields.e2e.ts`, `app/e2e/saveflows.e2e.ts`,
`app/e2e/recalc.e2e.ts`, store tests `reverting_an_ai_change_keeps_the_users_later_edits`
and `revert_refuses_when_the_user_changed_the_same_value`.

- Text fields (Job tab, job title): Escape restores, typing survives an AI
  save, an empty job name is refused, Back right after typing keeps the text.
- Revert of an AI step after the user's own edits keeps those edits or
  refuses with a reason (`revert_conflict`); it never silently drops them.
- A failed save stays "not saved" through an outside change; export after a
  failed save stops.
- Follow mode opens a job only for an AI write or `show_project`, never while
  the user draws, places, drags, types or has a dialog open.
- Undo, redo, AI save, merge on save, standard roof, face → building and move
  all recalculate (class 2).

## 7. Window size and language (@ui)

Guards: `app/e2e/shell.e2e.ts`.

- German or Slovak at 1360×860 and 1440×900 with the editor open: every
  toolbar button lies inside the window; a strip that scrolls sideways shows a
  scrollbar and scrolls with the wheel.
- Half-screen (≤ 960 px) library: double-click the lowest visible row; a click
  never moves the rows under the pointer.

## 8. Untrusted input and UI / MCP parity (@arch, @math)

Guards: core `only_regular_files_of_a_sane_size_are_attached`,
`check_limits_refuses_more_parts_walls_or_objects_than_a_real_job_has`, CLI
`attaching_the_servers_own_input_is_refused_and_the_session_goes_on`, MCP
`every_write_tool_requires_a_summary`, `app/src/lib/editparity.test.ts` with
`crates/tectora-core/tests/edit_parity.rs`, `tests/app_commands.rs`.

- MCP paths: devices, pipes, folders, private files, huge files are refused.
- Huge part / wall / object counts are refused before the core runs.
- The same editor operation gives the same value in the UI and over MCP
  (rounding at exact halves).
- Every Tauri command has a bridge counterpart reading the same arguments.
- Oversize windows, dormers off the roof and faces cut below zero carry a
  warning (`check_cases`, `result_report`); export works with dormers.

## 9. Concurrent saves and building-level edits (@errors, @math)

Guards: store `a_checked_save_*`, `saves_of_one_job_wait_for_each_other`,
`revert_all_keeps_the_users_edit_between_two_ai_steps`, MCP
`a_tool_change_does_not_overwrite_a_user_edit_the_app_saved_meanwhile`,
`a_building_height_sets_every_wall_in_one_save_and_changes_the_join`, E2E
saveflows "an AI save during the autosave", joined "the building height sets
every wall…", `app/e2e/results.e2e.ts`.

- An AI save landing between the editor's read and write survives (saves are
  compare-and-swap under `.activity/<id>.lock`); a revert never overwrites an
  autosave made while it merged.
- Revert all over AI steps with a user edit between them keeps the edit or
  refuses, never drops it.
- The building height sets every wall of the part (and breaks, half-hips,
  objects on it) in one save; both joined roofs recalculate; one undo
  restores it.
- Windows, chimneys and openings off every roof face carry a warning; the
  result is unchanged.
- The Results area rows add up to the total, hand-drawn faces included.

## 10. One building stops every roof (@math, @errors, @ux)

Guards: core `a_wall_of_no_length_leaves_only_its_building_out_and_says_where`
(walls up to 1 cm), `a_building_the_core_fails_on_alone_leaves_the_others_their_roofs`
(`tests/result_report.rs`), `tests/modell_budget.rs` (a wing 5 mm off and
turned 0.2° once aborted the process out of memory), vitest
`corners.test.ts` "drawn footprint without repeated corners" and "a corner
added on a wall", `errors.test.ts` "names the building whose roof fails on
its own", MCP `a_write_that_leaves_a_building_without_roof_says_so_and_calc_names_it`;
fuzz `cargo run --release -p tectora-core --example join_fuzz -- 2000 <seed>`.

- Corners closer than 1 cm (a corner clicked twice, the closing corner
  snapped onto the first, a typed 3,006 for 3) never reach the core: the
  drawing drops them, and a file that has them reports `zero_length_wall`
  for that building only (the core's own rule: a wall under 0.01 m is a
  point); the other buildings keep their roof.
- A corner added by double-click lies on the wall (projected, not rounded to
  the grid), and the double-click that ends a drawing adds none.
- A core error that a building also ends with on its own leaves only that
  building without a roof; the rest is calculated without it (a deliberate
  departure from DACAD's all-or-nothing run, Jakub 2026-11-14). Export and
  the stored result stay blocked while any building has no roof or any
  input is left out.
- The join stage has an edge budget (`modell::split::MAX_POLY_EDGES`): past
  it the fallback calculates with its warning instead of the process dying.
- A join of two buildings (touching, overlapping, different eave heights,
  overhangs up to 3 m) is calculated by the core or, where the port faults
  as the original does, by the fallback with its warning; never an error
  toast or a stale roof.

## 11. A building that is not closed in 3D (@ui)

Reviewers: ui. Guards: vitest `scene3d.test.ts` "a closed building in 3D",
"wallUnderRoof", "dormerFaces"; look at every roof from the four low corner
orbits, not only from above.

- The eave height is measured at the overhang line, so the roof passes over
  the wall line higher than the eave: every wall goes from the ground to the
  underside of the roof over its line (`scene3d.wallUnderRoof`), never to the
  eave or building height. Neighbouring walls meet at one height.
- The calculation's gable stands in the verge plane; beside a wall drawn up
  to the roof it is left out (`gableShownByWall`), or it hangs in front of
  the building.
- Walls end on the roof, so they sit further back in depth than the roof
  (polygon offset) and their top edges never show through it.
- Dormers one behind the other share cheek lines: each face belongs to the
  dormer whose front wall it touches (`dormerFaces`).
