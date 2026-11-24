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

Guards: `app/e2e/fields.e2e.ts`, `app/src/lib/numeric.test.ts`,
`app/e2e/audit-r2.e2e.ts` (dormer bar and dialog), `app/e2e/audit-r3-ed.e2e.ts`
(chimney / opening bar, quick steps, round-dormer segments).

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
- a field whose value the core checks first (dormer pitch and eave, window
  size): the bar's own ▶ / PageDown right after typing, and a dialog's OK
  right after typing. The value lands on the object it was typed for, OK
  waits for the check, and a refused value keeps the dialog open with the
  reason (the check goes through `pendingEdits.track`). Do it for every bar
  of that kind (dormer, chimney, roof opening, window), not only one.
- the same field with four quick ArrowUp presses and with a second field
  typed while the first check is still out (hold the check back with
  `page.route`): every step and both fields land, with no note. Do it for
  every field whose value the core works out: dormer bar, chimney / opening
  box (X, Y, width, depth, rotation), roof window, building height, wall
  length, the distances on the face (`audit-r4-ed.e2e.ts`).
- ArrowUp then ArrowDown, and a value typed and the old one typed back,
  while the first answer is out: the field and the file keep the last value
  the user set (`numeric.svelte.ts` `toSend`).
- an AI save that deletes an *earlier* item of the same list while the
  value is with the core (dormer 1 deleted, dormer 2 edited): the value
  lands on its own item and the bar follows it; the AI deleting or changing
  the *edited* item itself refuses the value with `editor.actionStale` and
  deselects it. Never the neighbour at the same index.

**One rule for all of these (`app/src/lib/jobedits.ts`, do not add a
per-bar variant):** an edit that waits for the core takes a *hold* on its
item at input time (`hold`, `holdRoof`), runs through `JobEdits.run` (one
after another, each on the job as the one before left it, only the typed
field taken from the user) and is refused only when an outside save changed
its own item. What an outside save changed is decided where the job is
replaced (`swapOutside` → `JobEdits.outside`) from the file before and after
the save, both in the core's form. Never compare an editor copy of an item
or of the job with a later one by `JSON.stringify` across an await: key
order and defaults differ from the file's (`half_hip: null`), and the
user's own earlier step changes it too.
- a dialog whose fields ask the core: OK asks once more for the object as
  it stands (a kind or shape change has no check of its own), and a
  refusal clears once the user types the field again

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
  deletes (Duplicate, Delete) acts once, also when the first click swaps
  the bar and a destructive button appears under the pointer (Close face
  → Delete face, `audit-r2-ui.e2e.ts`). Check it in German and at
  600×420 too: another button lands under the pointer there ("In Gebäude
  umwandeln", `audit-r3-ed.e2e.ts`); bars that replace another use
  `freshBar` (`dblclick.ts`).
- A double click on every action that waits for the core: OK of the
  building dialog (move with a copy), of the calculation helper, of the
  round roof and of the dormer window, Make it a building, Store result,
  the status select. It acts once and shows no note; the action is wrapped
  in `exclusive` (`inflight.ts`) and its button is off while it runs
  (`audit-r4-ed.e2e.ts`).
- A double click on a button that opens a dialog leaves the dialog open
  (the second press lands on the backdrop; `Modal.svelte` uses `shownNow`).

## 2. Every edit recalculates (@errors, @test)

Guards: `app/e2e/recalc.e2e.ts` (undo, redo, AI save, merge on save,
standard roof, face to building, move, rotate, mirrored copy, window /
chimney / dormer, flat roof on and off, delete part, Revert in the banner,
round roof, calculation helper; a move or rotation compares the roof lines,
since the area does not change).

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

Guards: vitest `i18n.test.ts` "tools that look alike", `app/e2e/objects.e2e.ts`
"a manual face is drawn neutral and labelled, not like a building".

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
(`wallUnderRoof`); manual faces appear in 3D; at any zoom the plan grid stays
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
- An edit that awaits the core and then writes (status change, store result)
  while an AI save replaces the project: it lands on the current project or
  is refused with a note, never on the discarded one
  (`audit-r2-status.e2e.ts`). An AI settings change revert that keeps the
  user's later value says so. The same for every editor action that awaits
  the core (typed distance, wall length, building height, Duplicate,
  Move / Rotate / Mirror, helpers, face to building): an AI save of other
  things lets it land, a change of its own object refuses it with
  `editor.actionStale` (`JobEdits.run`, `audit-r3-ed.e2e.ts`). Try each
  **after an ordinary edit** (Gable then Eave, Flat roof on and off), not
  only on a freshly opened job: the editor's copy then differs in form from
  the file, which once made every such action refuse falsely
  (`audit-r4-ed.e2e.ts`). What the action derives from the job (the new
  building's number, which part to select) must come from the job as it is
  when the core answers, not from before the await.
- Re-offer of a job that keeps an earlier stored result: a roof the core
  refuses keeps the job a draft and shows the refusal; the "keeps the
  earlier result" note names the figure that differs (a material, a line),
  never two equal areas. `offer.ts` `snapshotDifferences` and the core's
  `snapshot::compare` share `tests/fixtures/parity/snapshot_compare_cases.json`.
- Two programs changing `settings.json` at once (the app's first-start
  write and an AI client's `update_settings`): neither change is lost
  (`Settings::update`, one lock); a lost AI change is never shown for review.
- While a recalculation runs, Results, the status bar and the views mark
  the figures as pending (`calc-pending`), never show the old roof as current.
- Undo / Redo keep editor memory that a redo needs (the pitch Eave gives
  back after Gable, Undo, Redo).
- A failed save stays "not saved" through an outside change; export after a
  failed save stops.
- Follow mode opens a job only for an AI write or `show_project`, never while
  the user draws, places, drags, types or has a dialog open.
- Undo, redo, AI save, merge on save, standard roof, face → building and move
  all recalculate (class 2).
- A stored offer result an older engine made, opened from the library or by
  follow mode / `show_project`: one neutral notice leads to the history
  entry, which lists each figure old -> new; the result card says
  "recalculated <date> (Tectora a -> b)" with the offer's own date; the
  entry's button puts the offer's figures back for good
  (`app/e2e/audit-r8-ed.e2e.ts`, UX-702a-d).
- That recalculated offer, later (`app/e2e/audit-r9-recalc.e2e.ts`, round 9
  Design A): an AI save lands just before the press on "put the offer's
  figures back" (only the stored result changes, the AI's change stays;
  Revert on a stale entry answers `revert_stale`); the user stores a new
  result (no "recalculated" line, no button, also after a reopen); Undo
  across the put-back (line and button return); an AI save and its revert
  (the button stays; an opened entry stays open); an "engine" entry the
  library did not write itself (no line, no button); the job is left while
  it is read (no notice in the next editor); the notice's button at 800x600;
  line lengths changed and the area did not (the notice names the codes);
  the library after the open (still "changed by AI", the panel names Tectora).

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
- Unknown top-level keys: at most 64 KB and 32 levels of nesting (deeper is
  `too_many`, the job is not opened or imported; the change log and the
  app's `{ project }` argument wrap the job deeper than its file), and a key
  named `__proto__` is dropped on every read (JavaScript would set the
  prototype). Guards: `check_limits_refuses_deep_unknown_keys`,
  `deep_unknown_keys_go_through_the_command_wrapper_or_are_refused_on_open`,
  `every_change_log_line_of_the_deepest_allowed_job_parses_back`,
  `a_key_named_proto_is_dropped_from_unknown_keys_on_load`, `merge.test.ts`.
- The same editor operation gives the same value in the UI and over MCP
  (rounding at exact halves).
- Every Tauri command has a bridge counterpart reading the same arguments.
- Oversize windows, dormers off the roof and faces cut below zero carry a
  warning (`check_cases`, `result_report`); export works with dormers.
- An AI's reason shown in a dialog or card that grants or reports something
  (the folder request, the AI settings card) is quoted after "Reason given
  by <AI>:", never plain text between Tectora's own lines (UI-1604; E2E
  audit-r17-b).

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
  result is unchanged. Also while another building of the job has no roof:
  only objects nearest a failing building or inside its walls (a cross
  drawn as two rectangles) go unjudged (core
  `an_object_off_the_roof_is_warned_while_another_building_fails`,
  `an_object_inside_a_failing_building_is_not_judged`, E2E
  audit-r16-b HUNT-1502). Results counts them apart ("off the roof: n"),
  and every placement made before one calculation answers is checked; Undo
  takes its toast back (audit-r16-b UI-1502).
- A save or a revert that writes over a file another program made
  unreadable says "replaced" from the store's answer (`repaired`), never
  from a flag the reload set; the notice before it stays until then and the
  status bar does not say saved (store
  `a_write_over_an_unreadable_file_says_it_replaced_it`, E2E audit-r16-b
  ERR-1502, UX-1504, TEST-1504). The "replaced" notice names the job and
  outlives the editor (Back right after an edit, ERR-1602); the notice
  before it goes with the editor (TEST-1606). A newer Tectora's write over
  the open job gets its own lasting notice and status text, never "could
  not open" or "saved", and is never written over (UI-1603, HUNT-1604);
  the status bar fits at 960x540 in German (UI-1602). E2E audit-r17-b.
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

## 11. A roof that is not the core's result (@math, @errors, @test)

Guards: core `tests/roof_cases.rs` (the T house turned, with a window, with
a far garage; `a_small_jog_in_a_hipped_house_keeps_the_standard_roof`,
`hipped_l_and_hipped_wing_keep_the_standard_roof`,
`a_wing_a_few_centimetres_under_the_house_plane_is_never_a_lean_to`,
`a_doubled_face_fails_the_structure_check`,
`the_measured_wall_area_is_the_wall_stages`), `check.rs` unit tests for
`sound`, `wing_split` unit tests (cut budget), MCP
`calc_names_merged_planes_and_wing_joined_with_edges`.

- A departure (`merged_planes`, `wing_joined`, `not_calculated_by_core`)
  appears only where the core stops, or leaves a centimetre step, and only
  after `check::sound` accepted it. A roof the core calculates (hipped jog,
  hipped L, hipped wing) keeps its figures to the last digit.
- The same building turned by any angle and rounded to millimetres gives
  the same areas and lengths, and the same notice.
- A window, chimney, opening or dormer, or a second building standing
  clear, never changes which roof the building gets; a faulty object still
  ends in its own error.
- A wing only a few centimetres under the house plane is one face or an
  error, never a lean-to with overlapping faces.
- Many wings or hundreds of walls stay inside the budget (`MAX_CUTS`,
  `MAX_WALLS`, `MAX_SWEEP_WALLS`): an error within seconds, never a hang.
- A departed roof has a wall area, and an error names walls only where
  their parallel planes can be the cause.

## 12. A building that is not closed in 3D (@ui)

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

## 13. Labels of several buildings (@ux, @ui)

Reviewers: ux, ui. Guards: vitest `planlabels.test.ts` ("labels of several
buildings keep off roof lines and walls", "chooseLabels (UX-018)",
"wallNumber"), `app/e2e/planflow.e2e.ts` ("two buildings side by side",
"wall lengths keep off the ridge and face labels off the walls"),
`app/e2e/joined.e2e.ts` (UX-012). Put a second building 0.3 m beside the
first, touching it and drawn into it, at 1360x860 and 600x420.

- Wall numbers carry the building ("2.3") once there are several.
- No wall length sits on another building; the length of a wall facing the
  other building moves inside, and off the ridge or a hip when a spot along
  the wall is free (only a wall with no free spot keeps its middle).
- No face label overlaps a wall length or crosses a wall; a rotated label
  on a narrow face falls back to the pitch alone.
- In an AI review the change beside a wall ("40° → 35°") stays inside the
  plan and clear of the other labels.
- A selected dormer, chimney, window or opening at 600x420: its distances
  (from the eave, left, right) overlap neither each other nor a face label
  (`audit-r3-ui.e2e.ts` UI-209, vitest "placement dimension labels").

## 14. Lengths while drawing (@ux, @ui)

Reviewers: ux, ui. Guards: `app/e2e/planflow.e2e.ts` ("walls drawn so far
show their lengths while drawing", "a wall by length and typed angle").

- Every wall drawn so far shows its length (`draft-wall-dim`) and the wall
  under the pointer its running length, in the UI language's decimal format.
- A length typed in the draw bar, with an arrow key or an angle (absolute or
  from the previous wall), gives a wall of exactly that length; the label
  shows it.
- The labels stay readable in German and Slovak at 600x420 (the draw bar's
  angle field and the PlaceBar do not clip).
- A mistyped long wall (99 m) removed with the draw bar's undo: the plan
  fits the drawing that is left again (`audit-r3-ui.e2e.ts` UX-203).

## 15. Edits waiting for the core while the user keeps working (@ui, @errors, @ux)

Reviewers: ui, errors, ux. Guards: `app/src/lib/jobedits.test.ts`,
`app/src/lib/numeric.field.test.ts`, `app/e2e/audit-r4-ed.e2e.ts`,
`app/e2e/audit-r5-ed.e2e.ts`, `app/e2e/audit-r6-ed.e2e.ts`,
`app/e2e/audit-r7-drag.e2e.ts`, `app/src/lib/dragsession.test.ts`,
`app/src/lib/planpress.test.ts`. Hold the core's
answer back (Playwright `page.route` on the bridge, 0.3 to 3 s) for every
case, and assert that it really came while the pointer was down.

- A value typed on one item, then PageDown and the same value (or one arrow
  step) on the next item of the same size, lands once on each item; the
  field never shows a value the file does not have (UI-403).
- A width or height typed just before a drag of another object or of the
  building survives the drag; nothing is written back at the drop (ERR-401).
- A confirmed delete (corner, dormer, building) still happens after an AI
  save or the user's own edit landed while the question was open (UI-401).
- The user's own delete of an item whose edit is with the core ends quietly;
  only a change from outside gives the refusal, and the refusal names the
  object and stays marked on the field (UX-401, UX-402).
- Holding an arrow key shows the newest step, never an earlier one coming
  back (UX-403).
- A refused helper or round-roof dialog stays open with its values.
- A drag is one transaction (`app/src/lib/dragsession.ts`, rule 7 of
  `jobedits.ts`): from the press until its result is in the history the drag
  is the only writer of the job. Run the matrix of
  `app/e2e/audit-r7-drag.e2e.ts`: drag kind (object, building, corner) x
  phase (pressed and not moved, moving, drop held with the core) x intruder.
  - User input is refused, not replayed (owner decision, ERR-602): Delete,
    Ctrl+Z, an arrow, Tab + Enter on a bar button, a mouse click on a bar
    button during a touch drag, a typed value do nothing and nothing of them
    happens after the drop; the pressed object is the one that moves, never
    its neighbour (UI-603, UI-605, ERR-604). Escape cancels (the job goes
    back to the press, nothing is saved, the plan hint says so: UX-604), F3
    switches the view.
  - A second finger, the pen or a mouse button on an item, a roof line, a
    wall, a corner or the background during a touch drag (CDP
    `Input.dispatchTouchEvent`) is ignored: every `onpointerdown` of the plan
    goes through `press` (`planpress.test.ts`; ERR-501, UI-601).
  - What is not the user's waits for the end: an answer of the core (a
    corner drag holds the edits like an object drag, UI-502), an AI save of
    this job (reloaded once after the drop, both kept), follow mode (the
    editor stays on the job, also during a corner drag: ERR-601).
  - Undo right after a building's drop, its core move still out, undoes
    exactly the drop and Redo brings it back (ERR-603). A drop the core
    refuses is put back, its toast names the move, and an Undo pressed
    while it settled undoes nothing (ERR-701, TEST-701).
  - Ctrl/Cmd+, (App's settings shortcut, outside the editor) opens nothing
    during a drag (HUNT-702); an AI save during a drag that ends with
    Escape or a press that never moved is read then (TEST-702).
  - A press on a ridge, hip or valley line selects and drags the building
    under it (UI-701); a finger or pen held still on the plan opens its
    menu, + / − beside Fit zoom without a wheel (UX-703).
  - After every cell: the file holds exactly the drop, a value typed
    afterwards lands within 3 s, the status says saved, Back leaves; no
    autosave writes a half-dragged job; leaving mid-drag ends it where it is.
- Undo right after Enter undoes the value just typed, Redo brings it back
  (UX-501); the status bar says saving while an edit is with the core
  (UX-502; hold the answer until the sampling ends, never for a fixed
  time, TEST-706); a dialog's OK keeps the focus while it waits (UI-503); a
  dialog's own core checks run one after another (the dormer dialog).
- Revert, Revert all and the put-back of the offer's figures (AI tab and
  result card) are a short transaction like a drag (owner decision
  2026-11-23, design A1; retracts the round-10 rule "an edit typed while
  Revert waits is kept", which lost or resurrected edits: HUNT-1001,
  UX-1001, UX-1002, ERR-1004). A value being typed is stored and saved
  before the call; while the call is out (another process holds the job's
  lock) every edit is refused: same field + Enter, another field + Enter,
  a value typed without Enter, a confirmed building delete, Ctrl+Z; the
  status says "Vracím…". The answer is the job (no merge); the file and
  the screen agree whenever the status says saved (sample it every 50 ms,
  TEST-1003); the value typed afterwards lands within 3 s; a `busy` answer
  changes nothing and keeps the button (`coreaction.test.ts`,
  `app/e2e/audit-r11-ed.e2e.ts`, ERR-901 in `audit-r10-ed.e2e.ts`). Hold
  the lock until the intruder was tried, never for a fixed time. No key
  shortcut acts while the job is still being read (HUNT-908).
- One writer gate (round 12, fix design 1): the press of Revert / the
  put-back first waits, input still open and at most 3 s, for every edit
  still with the core and for the save, then claims the call in the same
  tick; past 3 s it answers `busy` under its own title, changes nothing and
  the editor stays usable (HUNT-1101, ERR-1102). Everything outside the
  editor's DOM refusal asks `anyWriterOpen()` / `gatedKeys()`
  (dragsession.ts): window keys (App's Ctrl/Cmd+, , the draw bar, PageUp /
  PageDown, dialogs, the context menu), the window presses that commit a
  field, toast actions (the press does nothing, the toast stays). `commit()`
  and `restore()` refuse while the call is out (`CoreAction.foreign`: throws
  in dev and tests, else drops the edit with a notice). Leaving waits for the
  answer; a failure is told also after the editor is gone and names the job
  (ERR-1105). `app/src/lib/gate.scan.test.ts` lists every window / document
  listener, toast action and `mayLeave` site: a new one fails until it is
  gated or marked passive with a reason. Intruders of
  `app/e2e/audit-r12-ed.e2e.ts`, each with the answer held at the browser
  (`hold()` of review.ts): `draw-enter`, `async-answer-status`,
  `async-answer-store`, `toast-action`, `settings-key`, `leave-open`
  (busy, a notice names the job), `ai-save` (the AI's change on screen =
  file, for revert, revert all and the put-back; TEST-1101),
  `presave-held-lock`. Limit: the preview bridge answers one request at a
  time (TEST-1102), so a command sent while the call waits for the lock is
  answered only after it; a class-15 guard therefore holds the answer at the
  browser (`hold()`, `page.route`) to make the order of the real app, never
  relies on the bridge's timing. Accept keeps the banner until the core
  answered (ERR-1103); the banner's buttons keep the keyboard's focus
  (UI-1103).
- The prepare phase ends only on a quiet editor (round 13, design A): the
  field focused at the press is stored once, before the loop, and never
  blurred by it (UI-1201); the claim waits while a field holds typed text, a
  confirm-then-edit is open (`asking`), the context menu or any dialog is
  open, and gives up after 3 s with `busy_edits` (the editor's own edits,
  not `busy`, which is the core's lock: UX-1201). Back pressed meanwhile
  waits for the whole press (`reviewPress`, ERR-1203); a press cut by a
  teardown says `left_before_action`. The menu's entries and `restore` refuse
  while the call is out, and a refused restore keeps the history whole. The
  backstop's release branch is `dropForeign` (coreaction.test.ts). Allowing
  an AI's folder request asks `mayLeave` before the switch (HUNT-1201).
  Intruders of `app/e2e/audit-r13-ed.e2e.ts`, each acting while the earlier
  edit's answer is held: `prepare-typing` (with and without Enter),
  `prepare-half-typed`, `prepare-menu`, `prepare-dialog`, `prepare-confirm`,
  `prepare-leave`, `own-slow-edit`, plus the folder request with a failed
  save, a footprint being drawn and a Revert out. `gate.scan.test.ts` counts
  listeners per file, receiver and event in every shape, requires the gate
  first in each gated handler, counts toast actions of every shape and lists
  every `openId =` and `libraryChanged` use in App.svelte with its reason.
- A toast action after its editor is gone (round 14, HUNT-1302): every
  editor toast whose button edits the job is one of the editor's own
  (`ownToasts`, closed in `onDestroy`) and names the job in its title; the
  panel's `current()` answers null once the editor is destroyed. Show the
  Job tab's "keeps the result stored earlier" toast, then: an unrelated edit
  (it stays), Back (it is gone at once, not by its timer: hover it first),
  Back and another job opened (no toast, the left job's stored result
  unchanged), the dock moved to Results (its button still stores).
  `app/e2e/audit-r14-ed.e2e.ts`.
- The prepare phase says what held it (round 14, UX-1301, TEST-1301): with
  the edits saved and only a menu, dialog or question open at the deadline
  the notice is `busy_open`, else `busy_edits`; a dialog opened and closed
  before the press holds nothing (`modalcount.ts`, its release counts
  once). Revert all refused is titled in the plural (`err.action.revertAll`,
  UI-1301). `gate.scan.test.ts` lists every `await confirm(` of the editor
  by its function (inside `asking`, or passive with a reason), requires
  `asking`'s `finally`, and refuses a spread inside a `notify(` literal
  (TEST-1303, TEST-1305). A test whose steps inside the 3 s budget are
  wall-clock sets the dev build's `window.__e2e.prepareMs` (TEST-1306).

## 16. Huge but valid jobs reach every view (@arch, @ui)

Reviewers: arch, ui. Guards: `store::tests::the_card_of_a_job_at_the_face_and_corner_limits_is_bounded`,
`thumbbox.test.ts`, `extent.test.ts` (scans app/src for spreads into
Math.min / Math.max and push), `planscale.test.ts` and the 200-building
test in `app/e2e/audit-r6-ed.e2e.ts` (the plan stays interactive, UI-501).

- A job at the limits (500 hand-drawn faces of 500 corners, 200 buildings)
  received as a bundle or saved by the AI: the library lists every job, the
  editor and the 3D view open it (ARCH-402).
- In such a job a click on a wall or an object only recolours the 3D view
  (`data-builds` on the 3D host stays the same; UI-602), and a zoom or pan
  step places only the labels in the window (`data-label-work` on the plan
  stays in the thousands; UX-602). Count work, not time: `planscale.test.ts`,
  `planlabels.test.ts`, and the 200-building and 500-face tests in
  `app/e2e/audit-r6-ed.e2e.ts`.
- A change log of large entries (notes near 1M characters, 50+ saves) stays
  at 50 entries and Revert all still returns to the version before the AI
  (ARCH-401).
- A small received bundle with a long history imports in under a second
  (HUNT-401).
- A pipe, link or device in place of a lock file, the change log or
  `settings.json` never hangs a save and never writes outside the folder
  (HUNT-402, HUNT-403); a `settings.json` with a byte order mark loads
  (HUNT-404).

## 17. An entry that only says what it is (@arch, @ux, @test)

Reviewers: arch, ux, test. Guards: store tests
`an_ai_summary_that_reads_like_a_revert_is_a_change`,
`an_entry_is_a_recalculation_or_a_put_back_by_its_fields`,
`a_revert_counts_the_changes_and_the_ai_steps_it_undid`,
`t1401_a_put_back_reverted_by_itself_is_undone_and_counted`; bundle test
`a_received_log_keeps_its_reverts_and_put_backs`; MCP tests
`a_summary_that_reads_like_a_revert_is_undone_as_a_change`,
`revert_to_a_put_back_by_itself_brings_the_recalculation_back`;
`app/src/lib/live.test.ts` (quoted AI words in all four languages, no
component calls `entrySummary`); `app/e2e/audit-r15-kinds.e2e.ts`.

- An AI summary worded like Tectora's own sentences (`Reverted: …`,
  `Reverted 2 changes since: …`, `Stored result recalculated (Tectora …`,
  `Offer result restored (Tectora …`), in English or already in the UI's
  language: the banner, the history, the library card and the top bar show
  it as the AI's words in quotation marks, and Revert / MCP
  `revert_last_change` undo exactly that save (ARCH-1401). What an entry
  is comes from its fields (`kind`, `undid_from`, `restores`, the engine's
  actor), never from its text.
- Revert all over a mixed log (the AI's own revert, the user's revert from a
  second Tectora on the same folder, the user's put-back, engine and
  received entries): the history line counts changes, the "reverted too"
  notice counts AI entries only, and a revert that writes nothing reports 0
  (HUNT-1401, TEST-1401, TEST-1402).
- A Revert button is drawn on the entry the core will undo (store.rs
  `undone_by`, the one walk for `latest_undone`, the review's steps,
  `undone_ai` and `ActivityItem.undone`): never on an AI's own revert, and
  after A1 A2 Ar(A2) on A1; the user's Revert of one of several AI steps
  keeps the others in review (UX-1501, UX-1505, UI-1501). A revert whose
  `undid_from` names no entry (a received log, a hand-edited one) is read
  by time and blocks nothing before it (HUNT-1501, DANGLING). The AI's
  words inside a revert sentence are quoted unless the undone entry is
  found and is Tectora's own (ARCH-1501). Guards: store tests
  `a_users_revert_of_one_ai_step_keeps_the_review_of_the_others`,
  `the_latest_change_and_the_undone_marks_follow_one_walk`,
  `a_revert_naming_no_entry_is_read_by_time`; bundle tests
  `a_received_log_from_a_clock_ahead_keeps_its_links`,
  `a_received_revert_whose_version_was_left_out_still_leaves_the_change_before_it`;
  MCP `get_review_keeps_the_steps_the_user_did_not_revert`;
  `app/e2e/audit-r16-review.e2e.ts`.
