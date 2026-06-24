# Multi-building design (DACAD-inspired composition)

Status: direction approved (user chose to adopt DACAD's model on 2026-06-24).
Supersedes the general "Stage 2 weighted straight skeleton" approach.

## Why

Reverse-understanding `DACAD CZ.exe` (the Czech/Eternit edition of the German
RISO-Star / ROOF-CAD roofing CAD) showed professional roofing software models a
design as **multiple parametric roof bodies** (templates: Saddle/Hipped/Pent/
Tent/Dutch-gable) plus dormers, each with per-side pitch/overhang — not a single
general weighted straight skeleton over an arbitrary polygon.

Adopting this **composition model** delivers BOTH outstanding user requests at
once and avoids the hard/risky general-skeleton geometry:
- "Multiple buildings/structures in one design" — directly.
- Flexible gables — each body uses the existing engine (rectangles compute exact
  sedlová/valbová/…; L/T/U hipped + Stage-1 wing-end gables). Complex roofs are
  composed from several bodies, the DACAD way.

**No new hard geometry.** Each building's roof is the existing
`core.roof_generator.generate_roof`. This work is data-model + UI + aggregation.

## Domain model

- New `Building` (dataclass): `footprint`, `pitch_deg`, `overhang_m`,
  `wall_height_m`, `name`. Footprints live in shared model coordinates, so
  multiple buildings are simply footprints placed at different positions — no
  per-building offset field needed.
- `Project` becomes a **container**: `project_name`, `units`,
  `buildings: list[Building]`.
  - **Backward compatibility is mandatory (258 tests must stay green).** Keep the
    legacy constructor shape `Project(project_name=, footprint=, global_pitch_deg=,
    overhang_m=, units=, wall_height_m=)` working: when called with a `footprint`
    (and no `buildings`), it creates a single `Building`. Expose legacy
    properties `footprint`, `global_pitch_deg`, `overhang_m`, `wall_height_m`
    that read/write `buildings[0]`. New code uses `project.buildings`.
  - Serialization: `to_dict` writes schema **2.0** with a `buildings` array.
    `from_dict` reads BOTH 1.0 (top-level single `footprint` → one building) and
    2.0. Round-trips losslessly.
- Use case: `calculate_design(project) -> DesignResult` = a per-building
  `RoofResult` list + aggregate totals (Σ area, Σ eaves, Σ ridge, …). Keep
  `calculate_project(project)` (computes `buildings[0]`) for existing callers.

## Persistence

`io.sqlite_library` and `io.project_file` serialize the buildings list via
`Project.to_dict` (v2.0). Old 1.0 entries load as a one-building project.

## GUI

- **Canvas** manages a list of footprints (one per building): draw a *new*
  building, select/edit/delete an existing one, with the active building
  highlighted. Every building's roof renders in 2D and 3D.
- **Right panel**: a building selector/list; the parameter controls (pitch,
  overhang, wall height, eave/gable tool) act on the *active* building. Per-
  building name.
- **Results**: per-building rows + a design total.
- **Excel**: per-building section + totals.

## Phasing (incremental, each independently tested; keep the suite green)

- **P1 — domain + application** (done by the orchestrator, not a subagent, due to
  the backward-compat delicacy): `Building`, `Project` container + compat,
  `calculate_design` + `DesignResult`, serialization 2.0 ←1.0. New unit tests +
  all existing tests green.
- **P2 — persistence**: sqlite + `.tectora` multi-building round-trip.
- **P3 — GUI canvas + panel**: multi-building draw/select/edit/delete, active
  building, per-building params, 2D render all.
- **P4 — 3D + results + Excel**: render all buildings; per-building + total
  results; Excel export.

## Out of scope (explicit)

- General weighted straight skeleton / gable on an arbitrary interior edge of a
  non-convex polygon. Replaced by composition (compose multiple bodies).
- Dormers (Gaube) — a possible later addition, mirroring DACAD.
- Shared/auto-merged roofs between adjacent buildings (each building is
  independent; abutting roofs are not auto-merged in this scope).
