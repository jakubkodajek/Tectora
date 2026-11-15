# ADR-0005: The editor calls the core for edit operations; TypeScript keeps only presentation

- **Status:** Proposed
- **Date:** 2026-11-15
- **Deciders:** Jakub Kodajek (to accept)

## Context

`AGENTS.md` says all roof logic lives in `tectora-core` and the UI and the
MCP server call the same `tectora_core::edit` operations. In fact about
1 700 lines of TypeScript in `app/src/lib` (`roofcalc.ts`, `dormers.ts`,
`transform.ts`, `openings.ts`, `roundroof.ts`, `corners.ts`, `manual.ts`)
were a copy of `edit/`, held together by a parity fixture
(`tests/fixtures/edit/parity.json`, `editparity.test.ts`) that covers only
the inputs someone listed. Every rule had to be written and fixed twice, and
copies outside the fixture had already drifted (`faceArea` and
`ManualFace::area` differed near 90°). Review finding ARCH-004.

Some of the copies answer during a pointer drag (the object bar following a
dragged chimney, the plan drawing a window), where a call to the core per
pointer move is not an option: the answer would come after the pointer has
moved on.

## Options considered

1. **Leave the copies and widen the parity fixture** - cheap, but every
   change stays a two-place change and the fixture stays a list of known
   inputs.
2. **Move everything to the core, also the drag-time functions** - the plan
   would lag behind the pointer, and the three.js and plan drawing would
   wait for an async call on every frame.
3. **Move every operation whose result is committed (a dialog's OK, a field,
   a click that creates something) to the core, in a few generic commands;
   keep in TypeScript only what must answer during a drag or while drawing,
   as presentation.**

## Decision

We choose option 3. The rule for what may stay: it draws or labels data the
core already calculated, or it must answer synchronously while the pointer is
down, and the result that is saved on release comes from the core.

Moved in this change (commands in `commands.rs`, mirrored in
`app/src-tauri/src/lib.rs` and `examples/ui_bridge.rs`, wrapped in `api.ts`):

- `roof_helper_context`, `roof_helper_preview`, `roof_helper_apply`
  (`edit::helper`): the calculation dialog. The MCP tool `roof_helper` calls
  the same `helper::run` and `apply_to_part`. The dialog sends what is typed,
  gets the figures back after a short pause (`lib/latest.ts`: the newest
  request wins, older answers are dropped, a failure is shown), and OK applies
  the same request through the core. `roofcalc.ts` is deleted.
- `round_arc` and the `round_roof` request of `roof_helper_apply`: the
  round-roof dialog's arc, default radius and the walls' values.
  `oppositeWall` and `spanBetween` are deleted.
- `item_geometry`: `reshape_opening` (a field of the object bar),
  `chimney_from_rect` (placing a chimney or opening), `face_area` (the
  hand-drawn face bar). `reshapeOpening`, `chimneyFromRect` and `faceArea`
  are deleted.

- `transform_part`, `offset_wall` (`commands.rs`, `edit::transform`): the
  transform dialog's OK, "Duplicate part" and the move a part drag ends on.
  `item_geometry` also answers `regular_polygon` (standard roof dialog) and
  `part_centre` (the dialog's default pivot). `offsetWall`, `regularPolygon`
  and `partCentre` are deleted.
- `item_geometry` `merge_dropped_corner` and `without_repeats`, new
  `edit/corners.rs` (also the one `corner_group` the MCP tool `edit_walls`
  uses; its former copy in the MCP crate is gone): a corner dropped on its
  neighbour, finishing a drawn footprint. `mergeDroppedCorner` and
  `withoutRepeats` are deleted.

Stays in TypeScript, and why:

| what | why it stays |
| --- | --- |
| `openings.ts`: `openingBox` | the object bar's fields follow a chimney while it is dragged; editing a field goes through `item_geometry` |
| `openings.ts`: `isConvex`, `nonConvexOpenings` | the 3D filter and the warning label of already-entered corners; the core refuses what it cannot calculate |
| `openings.ts`: `windowOutline`, `dormers.ts`: `slopeAt`, `hostHeightAt`, `roofHeightAt`, `isDormerFace` | read the calculated result to draw a window or snap a drag to the roof; no input of the model changes |
| `manual.ts`: `snapToRoof`, `roofPointList`, `lineLength` | snapping the pointer to roof points while drawing a hand-drawn line |
| `transform.ts`: `translation`, `rotation`, `mirror`, `power`, `mapPart`, `mapDormer`, `mapChimney`, `mapWindow`, `mapManualLine`, `mapManualFace`, `insideOutline`, `transformPart` | the plan draws a dragged part and `items.ts` moves and rotates objects with the pointer and the arrow keys, at once; what a part drag saves on release is the core's `transform_part` |
| `corners.ts`: `moveCorner`, `cornerGroup`, `onWall`, `samePoint` | drag of a wall corner, the corner added on a wall, the click on the last drawn corner |
| `finishDraw` in `Editor.svelte` | orients a drawn footprint counter-clockwise before it is added (the core normalizes on save as well) |

Not yet moved, although they are commit-time operations and should be
(follow-up, each with its parity cases removed together with the copy):

- `dormers.ts`: `defaultParams`, `buildDormer`, `parseDormer`,
  `dormerSection`, `eaveFor`, `solveDormer`, `pitchForBackWidth`,
  `nearestWallAngle` (DormerDialog, DormerBar, Editor; core: `edit::dormers`),
  and with them `roundroof.ts` (`arcFor`, `segmentsFor`,
  `MAX_ROUND_SEGMENTS`), which only the dormer code still calls. The dormer
  bar and dialog rebuild the dormer from its parameters on every keystroke and
  drag step and read it back synchronously in many places; moving them needs
  those two components restructured around asynchronous answers and is its own
  change.
- `types.ts` `buildingHeight` and Editor's `freezeHeight`: the wall bar's
  `patchEdge` / `applyAll` edit fields synchronously and commit before the
  press that changes the selection (lessons-learned 2026-11-08). Moving them
  needs an edit command with a serial queue (as `placeChain`) and is its own
  change. `RoofDefBar` also shows `buildingHeight` in a `$derived`.

## Consequences

- Positive: a calculation helper is one implementation; the dialog and the
  AI cannot get different roofs from the same values; the `faceArea` drift is
  gone; the helpers' DACAD cases are tested once, in `edit/calc.rs`.
- Negative / risks: the dialogs' figures arrive after a call (about 150 ms
  after the last keystroke); `openingBox` and the drag-time functions still
  have a TypeScript twin whose parity is only as good as `editparity.test.ts`
  and `openings.test.ts`. `openingBox` is not in the parity fixture.
- Follow-ups: the two items above. `openingBox` is checked by the core's own
  tests (`edit/openings.rs`) and `openings.test.ts`; the parity fixture
  `tests/fixtures/edit/parity.json` was not changed.

## Sources

- Review finding ARCH-004, `target/review/2026-11-14-f5d9531/arch.md`.
- `docs/architecture.md`, "Inside `tectora-core`" and the UI rule.
