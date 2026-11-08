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

No native number inputs: every number field uses `use:numeric`
(`app/src/lib/numeric.svelte.ts`); the unit test fails otherwise.

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
  name what each building is joined to
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
helper that turns them into geometry (e.g. `gableContour`) with the shape the
core actually returns for the case, taken from `tectora calc`.
