# Gables (štíty) on general roofs — Stage 1 design

Status: approved (user approved direction + staged scope on 2026-06-24; user granted
autonomous execution and waived their own spec-review gate for this round).

## Problem

Marking a footprint edge as `gable` only affects **rectangles** (the exact
calculator maps 2 opposite gables → `RoofType.GABLE` etc.). On **general shapes**
(L/T/U) the straight-skeleton path (`core/roof_from_skeleton.build_roof`,
`core/roof_faces.build_faces`) **ignores edge types** and always produces a fully
hipped roof. So a user who marks the front edge of an L as a gable sees only a
colour change (orange) and no geometric effect.

## Scope

Staged. **This spec = Stage 1 only.**

- **Stage 1 (this doc):** gable on a **terminal wing-end edge** — an edge that forms
  a *rectangular cap* (its two neighbour edges are perpendicular to it). Covers the
  user's case "gables on both ends of an L". Rectangles already work via the exact
  calculator and are unchanged.
- **Stage 2 (future):** gable on an arbitrary real edge (e.g. the long edge of the
  L's lower rectangle). Needs a true gabled/weighted straight skeleton. Out of scope
  here. Stage 1 must **reject** such placements gracefully (never emit a broken roof).

## Key invariant (drives the geometry + tests)

At a uniform pitch, a hip roof and a gable roof over the **same footprint** have the
**same total roof surface area** (both planes project onto the full footprint:
`area = footprint_area / cos(pitch)`). Gabling only changes the *breakdown*
(hip→rake, ridge extends, one cap face becomes a vertical wall), not the total roof
area. The vertical gable wall (štít) is a **wall, not a roof face**, so it is excluded
from roof area.

## Geometry — face-folding (variant A)

Compute the normal hipped straight skeleton as today, then for each `gable` edge:

1. **Validate** it is a rectangular cap (see `is_valid_gable_edge` below). If not →
   raise `GableNotSupported(ValueError)`.
2. Find the terminal **triangular cap face** of that edge: the single skeleton node
   `P` (offset height `t_P`) joined by skeleton arcs to **both** edge endpoints
   `A=V[i]`, `B=V[i+1]`.
3. **Fold:** move `P` horizontally to `M`, the foot of the perpendicular from `P`
   onto segment `AB` (the midpoint for a symmetric cap), keeping its height `t_P`.
   - Triangle `A–B–M(@height)` is now **vertical** → the gable wall (štít).
   - Arcs `A–P`, `B–P` become `A–M`, `B–M` = **rake** edges (classified gable/verge,
     not hips).
   - The ridge that ran `P→Q` (interior) now runs `M→Q`, i.e. the ridge extends out
     to the gable edge.
4. **Recompute** classification/areas/lengths from the modified node set:
   - Exclude the vertical gable triangle from roof faces (`build_faces`).
   - `A–M`, `B–M` add to gable/rake length; remove from hip length/count.
   - Eave perimeter excludes edge `AB`.

Exact for rectangular caps (Stage-1 scope). The two adjacent long-side faces remain
planar after the fold for a rectangular cap.

### `is_valid_gable_edge(footprint, i) -> bool`

True iff edge `i` (`A=V[i]→B=V[i+1]`) is a rectangular terminal cap:
- `A` and `B` are both convex corners, and
- the previous edge (`V[i-1]→A`) and next edge (`B→V[i+1]`) are both perpendicular to
  `AB` (within tolerance) and point to the same side of `AB`, and
- the straight skeleton gives exactly one apex node joined to both `A` and `B`
  (a single triangular cap face).

Pure domain function (no UI). Used by the core geometry, the GUI editor (to accept or
reject a click), and the CLI.

## Data model

No schema change. Edge types `eave`/`gable` already exist and already persist to
SQLite and `.tectora`. Stage 1 only adds behaviour + the validation predicate.

## Results (výkaz výměr)

- Hip count/length **decreases** at a gabled end; gable/rake (`gable_edge_*`) length
  **increases**; eave perimeter **decreases** by the gable edge.
- Total roof area unchanged (see invariant). Gable wall not counted as roof area.

## 2D rendering

- Gable edge stays orange (already). Add the two rake edges in the gable colour and
  draw **no hip lines** to that end. Recompute live on toggle.
- If the user toggles an edge that is not a valid Stage-1 gable, the editor shows a
  toast (new i18n key, e.g. `gableNotHere`) and leaves the edge as eave — never emits
  a broken roof.

## 3D rendering

At a gabled wing-end, render a **vertical triangular wall** (štít) instead of the
sloped hip plane, and the ridge reaching the wall apex.

## Tests (TDD)

Domain / core:
- `is_valid_gable_edge`: true for an L wing-end edge; false for a long/inner edge and
  for a reflex-adjacent edge.
- Golden: `build_roof(rectangle_with_2_opposite_gables, pitch)` (forced SS path)
  matches `calculate(RoofSpec(GABLE, …))` on total area, ridge length, hip count (0),
  and rake length.
- L with one wing-end gable: total roof area == footprint_area/cos(pitch) (invariant);
  hip count drops by the folded corners; rake length > 0; gable wall excluded from
  `faces` roof area; both ends of an L gabled simultaneously works.
- Invalid gable placement raises `GableNotSupported`.

GUI:
- Toggling a valid wing-end edge to gable then calculating produces a result with rake
  (no crash); toggling an invalid edge shows the toast and stays eave.

## Implementation plan (subagent tasks)

Dependency order matters — the core fixes the **shape of the output**, so it must land
first and alone; the renderers depend on it.

- **T1 — core geometry (first, solo).** `is_valid_gable_edge` + `GableNotSupported` in
  the domain; face-folding in `core/roof_from_skeleton.build_roof` and
  `core/roof_faces.build_faces`; results breakdown (hip→rake, eave, area). Full core
  test coverage incl. the golden + invariant tests. Must keep the existing suite green.
- **T2 — 2D + validation/UX (after T1, parallel with T3).** Editor: clicking a gable
  edge validates via `is_valid_gable_edge`; valid → toggle + live recompute; invalid →
  toast (`gableNotHere`, cs/sk/en) and stay eave. Canvas: draw rake edges + no hips at
  a gabled end. GUI tests.
- **T3 — 3D wall (after T1, parallel with T2).** `gui/editor/view_3d`: vertical
  triangular gable wall + ridge to apex at gabled ends. GUI test.

Each subagent gets a precise, self-contained prompt (as with FX-A/B). Verify every
subagent by reading its diff + running the full `QT_QPA_PLATFORM=offscreen pytest -q`.
