# Roof core verification

How the roof core (`crates/tectora-core`, DACAD core port `dcore`) is checked,
and what the checks found. Two independent references are used: DACAD's own
output, and geometry derived from first principles.

## Layers

| Layer | Where | Reference | Runs |
|---|---|---|---|
| DACAD captures | `examples/dcore_check.rs` on the RD2/SO2 captures in the project files | DACAD's own SO2, byte for byte | by hand |
| DACAD fixtures | `tests/dacad_cases.rs`, `coresol_cases.rs`, `modell_cases.rs`, `dcore_*.rs` | line and area totals DACAD calculated | every `cargo test` |
| Closed forms | `tests/analytic.rs` | hand-derived formulas | every `cargo test` |
| Geometric invariants | `tests/invariants.rs` | lower envelope of the roof planes, tiling, invariances | every `cargo test` (400 cases), nightly 20 000 |
| DACAD-order properties | `tests/properties.rs` | invariances DACAD keeps | every `cargo test`, nightly |
| Fuzzing | `invariants.rs` (polygon soups, mutated project JSON), `properties.rs` (DAD bytes) | no panic, finite output | every `cargo test`, nightly |
| Census | `invariants.rs::census_of_area_artefacts` (ignored) | exact envelope | by hand |
| Plausibility warnings | `src/check.rs`, `tests/check_cases.rs`, `invariants.rs::roofs_leaving_their_outline_are_flagged` | walls, overhangs, the faces' own corners | every `cargo test`, nightly |

### Closed forms (`analytic.rs`)

Each test sweeps a grid of dimensions, pitches, eave heights and overhangs
and compares every line total, the face areas, the ridge height and the face
count with formulas derived from plane geometry:

- gable, hip, pyramid on regular 3- to 8-gons, hip with two pitches,
- asymmetric gable (pitches and eave heights per side),
- mansard (pitch break on every wall) and gambrel,
- half-hip (Krüppelwalm) on both gables,
- L, T, U and plus-shaped hip roofs (hips and valleys at 45°),
- a chimney in a slope (area loss, WV, WH, WS) and a roof window,
- scaling and translation.

Every roof must also pass `support::check_structure`: planar faces at their
pitch, face area equal to its 3D polygon, faces tiling the eave outline, every
face side on a roof line and every line on a face side, and (without gables)
face sides = 2 × inner lines + outer lines.

About 2 000 roofs; all match to 1 mm + 2·10⁻⁵ relative.

### Invariants (`invariants.rs`)

- On a convex outline with one eave height (no overhang, or one pitch and one
  overhang) the roof is the lower envelope of its edge planes. The reference
  clips the eave outline to where each plane is lowest, independently of the
  engine.
- One pitch on an outline with notches: area = plan of the eave outline / cos.
- Walls pushed out by the overhang with no overhang give the same roof.
- Raising every eave by dz lifts the roof by dz and changes nothing else.

Inputs are generated as DACAD stores them (millimetres, hundredths of a
degree). Random roofs are compared with DACAD's tolerances in mind
(`Fit::Dacad`): its core merges points within 2 cm (POINT_EPS) and takes
events within 1 cm of height as one sweep level, so a corner may move by up
to 2.5 cm and a face's reported area (taken from its own edge list) may differ
from its polygon by a sliver along the outline. Roofs whose exact corners lie
within those tolerances of each other are skipped by the envelope properties
(`well_conditioned`); the census measures them instead.

## Results (2026-11-02, branch `claude/project-thread-je0695`)

- DACAD captures: dacad-core 106/106, s1 441/441, s2 809/809 SO2 byte-identical
  (477 of them with zero and non-zero overhangs mixed on one roof).
- Closed forms: all 16 tests pass.
- Invariants and fuzzing: pass at 20 000 cases per property.
- Census, 3 × 3 000 random convex roofs (pitch 15–70°), relative area error of
  the reported area against the exact envelope:

  | overhangs | ≤ 1e-5 | ≤ 1e-4 | ≤ 1e-3 | ≤ 3e-3 | ≤ 1e-2 | ≤ 3e-2 | > 3e-2 | DACAD error |
  |---|---|---|---|---|---|---|---|---|
  | none | 2858 | 21 | 18 | 11 | 2 | 0 | 0 | 90 |
  | one pitch + overhang | 2875 | 6 | 24 | 9 | 1 | 0 | 0 | 85 |
  | mixed | 2809 | 21 | 40 | 17 | 5 | 1 | 1 | 106 |

  "DACAD error" is the engine reporting DACAD's own calculation error
  (E00002), as DACAD does.

## Findings

1. **Mixed overhangs follow DACAD's model, not the plane envelope.** DACAD
   keeps an overhanging wall vertical up to its wall top (eave + overhang ·
   tan pitch) before its plane moves in (`dcore::s1::hidden_height`). With
   wall tops at different heights the hips bend at those heights and the roof
   is not the lower envelope of the planes; in one case (C4) the area is 4.5 %
   above the envelope. This is DACAD's behaviour: 477 mixed-overhang captures
   are byte-identical. With one overhang and pitch, or no overhangs, the roof
   is the envelope.
2. **Hips outside the building (C1, C3), DACAD's own.** About 1 in 3 000
   roofs with mixed overhangs and pitches gets hips running outside the eave
   outline or overlapping faces (C1: 60.35 m² on a 35.2 m² roof). Run in the
   DACAD core emulator, both are byte-identical to DACAD, so the port is
   faithful. The engine keeps DACAD's result and warns (see below).
3. **Reported area vs. face polygons (C2, C5).** DACAD takes each face's area
   from its own edge list, not from the merged corners. Mostly the difference
   is under 0.3 %, in C2 the total is 1.2 % above the exact roof while the
   polygons are exact (one face 8 % too big).
4. **Precision far from the origin.** The core works in absolute float32
   coordinates like DACAD's: an 82.86 m² L-shaped roof loses 0.05 % at
   (−120, 260) and 1.3 % at (0, −2500); lines stay within a few millimetres
   (`analytic.rs::far_from_origin_precision`). Shifting the project to the
   origin before the calculation would remove this, at the cost of byte parity
   with DACAD for such files.
5. **Huge coordinates (fixed).** A finite but huge value (x = −1e308 in a
   hand-edited project) passed validation, could not be written to the RD2,
   and overflowed in the fallback sweep: a panic in debug builds, NaN
   geometry in release. `Project::validate` now rejects coordinates,
   heights and overhangs beyond ±1 000 km (`model::MAX_COORD`), so such a
   part gives no roof (`huge_coordinates_are_rejected`).
6. **Face orientation.** `RoofFace::polygon` is documented as
   counter-clockwise, but faces come in either orientation (as in DACAD's
   SO2). The 3D view draws both sides, so nothing shows; the doc comment is
   wrong.

7. **Merged apex (P16, PR #16 CI).** Where four planes nearly meet at
   the top, DACAD takes their events as one sweep level and merges their
   corners, so a 45° face's corner is its neighbours' point, 1.16 cm off
   its own plane. SO2 byte-identical to DACAD's. `properties.rs`
   `faces_lie_in_their_edge_planes` now allows `slope × POINT_EPS` (2 cm in
   plan) on top of 1 cm.
8. **Single-line faces (F5, F6, F8, G6).** Also with one overhang, where a
   short wall meets its neighbours at a sharp angle DACAD writes a face
   bounded by a single line: two of its three corners coincide, its
   polygon has no area, its reported area is a 0.004–0.016 m² sliver. SO2
   byte-identical to DACAD's; `faces_are_flat_at_their_pitch` skips such
   faces.

All five are fixed cases in `tests/fixtures/check` (project JSON, the RD2
the engine writes, DACAD's SO2 from the core emulator), checked by
`properties.rs::merged_apex_corner_is_within_dacad_tolerance` and
`single_line_faces_are_dacads`. `properties.rs` now runs a fixed seed like
`invariants.rs`, so a pull request's checks do not depend on which rare roof
a random seed draws; the nightly run adds a new seed each night.

Every deviation from exact geometry found so far (73 roofs: the census's
worst 60, C1–C5, D1–D7 from property failures under other seeds) was run in
the DACAD core emulator and is SO2 byte-identical to DACAD. The property
tests run with a fixed seed (`invariants.rs::SEED`) so CI does not depend on
which rare DACAD artefact a random seed hits; `PROPTEST_RNG_SEED` explores.

The cases (project JSON, the RD2 the engine writes, DACAD's SO2) are in the
project files under `core-tests/dacad-candidates/`, the census's worst cases
under `core-tests/census/`. To check one in the DACAD core emulator:
`core_run.py C1_hips_outside.RD2 OUT/`, then
`cargo run --release -p tectora-core --example dcore_check -- OUT/C1_hips_outside.RD2`
with DACAD's SO2 next to it.

## Plausibility warnings

The engine stays 1:1 with DACAD, including the faulty roofs above. Instead
of changing the result, `tectora_core::check` measures every calculated
roof against its walls and the app and MCP show a warning (`RoofGeometry::warnings`):

| Warning | Measure | Tolerance |
|---|---|---|
| `outside_outline` | largest distance of a face corner outside the eave outline (each wall's strip out to its overhang, mitred; for joined roofs every building's) | 5 cm |
| `overlapping_faces` | plan area where two faces pass through each other or coincide; a face lying above another one (an overhang over a lower roof) is sound | 0.25 m² |
| `area_mismatch` | relative difference of the reported face areas (plus deductions) to the faces' own 3D polygons; single roofs only, joined roofs book their pieces by DACAD's own rules | 1 % |

Tolerances were set on 2 779 roofs: every DACAD-calculated `.DAD` in the
project files (golden, samples, mass s1/s2, mr3, core-stress, variants), the
census's worst 60 and the candidates. Sound roofs stay below 1 cm outside
and 0.06 m² overlap; 13 roofs get a warning, all of them geometrically
wrong:

- outside and overlapping: C1 (and the same roof in the census), mass s2
  R2_0152, R2_0250, R2_0909 (faces of up to 7 000 m² on a 150 m² building);
- overlapping only: C3, C4 (DACAD's mixed-overhang model folds two faces
  through each other; the same roof with the walls moved out is sound),
  mass s2 R2_0236;
- area only: C2 (and the census's copy), the near-pyramid hip S02 1000 m
  from the origin in three variants (10–26 %, single precision).

`roofs_leaving_their_outline_are_flagged` checks on random mixed-overhang
roofs, against the convex outline's own half-planes, that every roof more
than 10 cm outside is flagged and no roof within 3 cm is (120 000 cases over
six seeds pass). The closed-form and invariant tests require that no sound
roof gets a warning.

## Running

```sh
cargo test --release -p tectora-core --test analytic --test invariants --test check_cases
cargo run --release -p tectora-core --example check_scan -- FILE.DAD|PROJECT.json...
PROPTEST_CASES=20000 cargo test --release -p tectora-core --test invariants
CENSUS_CASES=3000 TECTORA_CENSUS_DIR=out \
  cargo test --release -p tectora-core --test invariants census -- --ignored --nocapture
```
