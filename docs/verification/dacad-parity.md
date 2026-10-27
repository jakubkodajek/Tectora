# DACAD parity — verification plan and findings

Goal: Tectora's roof quantities must equal DACAD's for the same input.

## Sources of truth

| Source | What it gives | Status |
|---|---|---|
| `morluto/rea` | Generic reverse-engineering MCP toolkit (Ghidra/Hopper bridge). Contains **no** DACAD formulas; usable only as a tool to decompile `DACAD DE.exe`. | not needed so far |
| `DACAD.HLP` (German WinHelp, 2003) | Input model, line types, rounding, norm rules. Decoded with a WinHelp/Hall-phrase extractor. | read |
| `Beispiel/*.DAD` (34 sample projects, 1994–98) | Input **and** DACAD-computed results (every line with type and length, every face area, group totals). | decoded, see below |
| `tools/dacad_testcases.py` | 14 single-rule projects. | calculated by DACAD itself (see below), fixtures in `tests/fixtures/dacad_cases/` |
| `tools/dacad_wine_calc.sh` | Runs the real `DACAD DE.exe` headless under Wine: opens a file, calculates, saves. Any new case can be checked against DACAD without Windows. | working |

## DACAD's input model (from the help file)

* Every footprint wall carries its own roof definition: pitch, eave height
  (Traufenhöhe), eave overhang (Traufenvorsprung, **measured horizontally**),
  material (tiles / sheet / none), priority, roof number.
* **Traufenhöhe is the height of the eave edge itself**, i.e. at the outer
  line after the overhang, not at the wall. Evidence: in TRAING04/06/08 every
  stored T line lies exactly at the stored eave height of its wall, for walls
  with overhang 0, 0.2, 0.4 and 0.45 m.
* Zero-length walls occur in the files. They are not inert: dropping the
  two zero-length walls (overhang 0.2 between walls with 0.45) from TRAING08
  changes DACAD's own result (K 18.646 → 22.571 m). A dormer's side walls
  are zero-length too and are oriented only by their stored direction.
* A wall can stack several definitions (up to 5 in `E008.DAD`): a Krüppelwalm
  is a vertical gable (90°, no material) up to the Krüppel height, then a hip
  plane with its own pitch; a pitch break (Knick) works the same way.
* A gable is pitch 90° + material "none": the face is computed but excluded
  from the roof area.
* Line types: T Traufe, F First, G Grat, K Kehle, P Pult, OL/OR/OS Ortgang
  left/right/oblique, S Schlucht, WH/WS/WV wall abutment back/side/front,
  c Knick.
* Output rounding: 2 decimals for lengths and areas (configurable).
* Norm groups (optional): DIN 18338 (openings ≤ 2.5 m² and lines ≤ 1 m not
  deducted), ÖNORM B 2219 (≤ 4 m², ≤ 2 m, plus "Beideckung": +0.5 m² per m of
  eave/ridge/verge/abutment, +1 m² per m of hip/valley/gorge), SIA 124 (≤ 1 m²).

## Golden corpus

`src/tectora/io/dacad_dad.py` reads both file formats. Check of the decoding:
in all 34 samples the decoded line lengths per type add up to the group totals
DACAD stored (`tests/io/test_dacad_dad.py`, run with `TECTORA_DACAD_DIR`).

Every sample mixes pitches, eave heights, Krüppelwalme and dormers, so none is
representable by a single-pitch engine. They are the acceptance set for the
per-edge engine; the single-rule cases localise failures.

```
python tools/dacad_golden.py export  "$TECTORA_DACAD_DIR" out/golden
python tools/dacad_golden.py compare "$TECTORA_DACAD_DIR" \
    --engine "cargo run -q -p tectora-cli -- calc {file}"
# Files DACAD re-saved as V2, engine fed the original V1 inputs:
python tools/dacad_golden.py compare calculated/ --inputs originals/ --engine ...
```

Tolerance: 0.006 m / 0.006 m² (half a unit of DACAD's 2-decimal output plus
float32 noise in the stored totals).

## DACAD under Wine

`DACAD DE.exe` runs on Linux with Wine 9 (32-bit), Mesa GL and Xvfb, given its
`SftTv32.DLL` and the licence key in the Wine registry. `tools/dacad_wine_calc.sh`
opens each file, runs the calculation (Ctrl+B) and saves; DACAD writes the
results back (as a V2 file). The 14 single-rule cases were produced this way.

| Case | DACAD lines (m) | DACAD roof area (m²) | Rust engine 4569e5f |
|---|---|---|---|
| C01 gable 10×8 40° | T 20, F 10, OL/OR 10.443 | 104.433 | match |
| C02 gable, overhang 0.5 on all walls | T 22, F 11, OL/OR 11.749 | 129.235 | match |
| C03 gable, overhang 0.5 on eaves only | T 20, F 10, OL/OR 11.749 | 117.487 | match |
| C04 hip 12×8 35° | T 40, F 4, G 25.249 | 117.194 | match |
| C05 hip, overhang 0.6 | T 44.8, F 4, G 29.036 | 148.251 | match |
| C06 pyramid 8×8 45° | T 32, G 27.713 | 90.510 | match |
| C07 shed 10×6 25° | T 10, **P 10**, OL/OR 6.620 | 66.203 | top edge reported as F, verges 18.84 |
| C08 half-hip, Krüppel from 5.5 m at 60° | T 30, F 10.268, G 9.165, OL/OR 7.071 | 137.286 | no Krüppelwalm (T 24, G 0, 135.76 m²) |
| C09 hip, sides 30°, ends 50° | T 40, F 8.124, G 20.035 | 117.068 | match |
| C10 hip, north eave at 3.5 m | T 38.572, F 3.286, G 25.249, **OL/OR 0.872** | 117.194 | step reported as W 1.74 instead of OL/OR |
| C11 L-hip | T 40, F 12, G 15.781, K 3.156 | 78.130 | match |
| C12 L with gable ends | T 32, F 16, G 3.156, K 3.156, OL/OR 4.883 | 78.130 | match |
| C13 gable, 30° / 45° | T 20, F 10, OL/OR 9.998 | 99.975 | match |
| C14 mansard hip, 60° to 5.0 m, then 30° | T 40, F 4, G 27.713, **c 30.762** | 145.386 | no pitch break (192 m²) |

What the cases settle:

* A gable wall's overhang extends the verge (C02: ridge 11 m, area as if the
  footprint grew by 0.5 m on every side).
* The half-hip roof is the Krüppelwalm model: 137.286 m² equals the value
  derived from the help file's description (137.29 m²). The horizontal lower
  edge of the Krüppel plane counts as T (2 × 3 m on top of the 2 × 12 m eaves).
* Eaves at different heights on one roof leave short OL/OR verges, not wall
  abutments (C10).
* A pitch break line is type `c` (Knick) and the hips run through it (C14).
* Verge sides (C15–C17, from the stored line endpoints): OL is the verge on
  the left, OR on the right, looking at the plane from outside its eave. In
  C16 (east eave 3.5 m) the 0.872 m step at the north end is OL (it belongs to
  the north plane, whose left is east), the one at the south end OR. A verge
  that is not square to the eave in plan is OS, not OL/OR: in the trapezoid
  shed C17 the slanted east verge (10, 0, 3) → (8, 6, 5.80) is OS 6.916 m, the
  square west verge OL 6.620 m, the top edge P 8.0 m.
* Horizontal priority (C18–C23, geometry of DACAD's `PRIORITY.DAD`). It is
  stored per definition: V1 wall record f32 at offset 36, V2 definition
  field 6. Without priorities DACAD keeps the intersection line at the
  steeper face (help: "wählt das Berechnungsmodul immer diejenige
  Schnittlinie, die bei der steileren Dachfläche liegt"):

  | Case | Priority 1 on | F | G | c | area |
  |---|---|---|---|---|---|
  | C18 notch, back wall 50° | none | 3.000 | 15.275 | 2.000 | 44.632 |
  | C19 | back wall (steeper, same as default) | 3.000 | 15.275 | 2.000 | 44.632 |
  | C20 | the two 30° walls beside it | 2.611 | 16.049 | 2.121 | 45.126 |
  | C21 bump, front wall 20° | none | 3.360 | 14.709 | 2.589 | 47.657 |
  | C22 | front wall (flatter) | 3.000 | 15.275 | 2.000 | 48.008 |
  | C23 | the two 30° walls beside it (steeper, same as default) | 3.360 | 14.709 | 2.589 | 47.657 |

  The horizontal intersection line is reported as type `c`. A 20° back wall
  in a notch (instead of a bump) makes DACAD itself stop with
  "E0002: Kein Durchstoßpunkt durch XY-Ebene für P1".
* A line rising along a wall from a reflex (inside) footprint corner is K,
  not a verge: the 1.462 m line on the notch back plane in C18–C20
  (identified by the Rust engine against these fixtures).
* Openings on the C04 hip (117.194 m² without openings, no norm group set):

  | Case | Opening | Deducted face | Tile area | Lines added |
  |---|---|---|---|---|
  | C24 | window 1.1 × 1.1 | −1.210 | 115.984 | none |
  | C25 | window 0.8 × 1.4 | −1.120 | 116.074 | none |
  | C26 | window 0.5 × 0.5 | −0.250 | 116.944 | none |
  | C27 | chimney 0.6 × 0.6 in the south plane | −0.439 | 116.755 | WV 0.6, WH 0.6, WS 2 × 0.732 |
  | C28 | chimney 0.6 × 0.6 across the ridge | −0.220 × 2 | 116.755 | WV 2 × 0.6, WS 4 × 0.366; F 4.0 → 3.4 |

  A window's width and height are taken in the roof plane (deduction
  w · h); its stored x, y are the plan position of its lower (eave-side)
  corner at the plane's start side. Checked on the C25 plan drawing: the
  0.8 × 1.4 window spans x 6.0–6.8 and y 1.50–2.65 in plan
  (1.4 · cos 35° = 1.147). A chimney is a plan polygon:
  its deduction is the plan area over cos(pitch) (0.36 / cos 35° = 0.439), WV
  is its downhill side, WH the uphill side, WS the sloped sides, and the
  ridge is cut where the chimney crosses it. V1 storage: window `0x2012`
  (x, y, w, h) after its wall, counted in the wall record's last byte;
  chimney `0x13` (u16 id, f32 height, u16 corner count) followed by `0x1013`
  corners (x, y), counted in `0x10`.
* Oblique chimneys (C32–C36, rotated 2°, 10°, 30°, 40°, 50°): every side
  is WS. WV and WH occur only for sides that are horizontal in the roof
  (parallel to the eave, C27); even 2° makes all four sides WS.
* Dormers (C29–C31) are separate roofs (help: "Gauben werden generell als
  eigenes Dach"), cut against the main roof. V1: walls with roof number 2
  plus a `0x11` record (roof 2, 1, 3 as in TRAING01); the side walls have
  zero length and only their direction orients their plane.

  | Case | Dormer | DACAD lines (m) | Tile area |
  |---|---|---|---|
  | C29 | gable, 2 m wide, 45°, eave 4.0, front on the eave wall | T 24.384 (incl. a **−2.0** T: the main eave under the dormer front), F 14.384, K 3.699, OL/OR 11.857, WS 3.111 | 125.708 |
  | C30 | same, front 1 m behind the wall | T 24.384, F 13.384, K 3.699, OL/OR 11.857, WV 2.0, WS 0.501 | 125.490 |
  | C31 | shed, 3 m wide, 15°, eave 5.0, front 1 m behind the wall | T 27.0, F 12.0, OL/OR 12.548, WV 3.0, WS 5.307, c 3.0 | 123.672 |

  The dormer's front wall against the main roof is WV, its cheeks WS, the
  valleys of a gable dormer K, and the line where a shed dormer meets the
  main roof `c`. The dormer's own eaves and verges count as T and OL/OR.

  More dormers (C39–C42):

  | Case | Dormer | DACAD lines (m) | Tile area |
  |---|---|---|---|
  | C39 | hip dormer, 45°, eave 4.0, front 1 m back | T 26.384, F 12.384, G 3.464, K 3.699, OL/OR 10.443, WV 2.0, WS 0.501 | 125.490 |
  | C40 | C30 with 0.3 overhang on every dormer wall | T 24.984, F 14.041, K 4.808, OL/OR 12.282, WV 2.0, WS 1.434 | 126.986 |
  | C41 | gable dormer across a hip line of the C04 hip (x 2–4, eave 3.8) | T 40.285, F 5.571, G 25.249, K 4.020, OL/OR 1.414, WV 2.0, WS 0.348 | 117.526 |
  | C42 | gable dormer through the ridge (front y 2, eave 6.5) | T 28.342, F 15.363, K 3.699, OL/OR 11.857, WV 2.0, WS 5.668 | 125.921 |

## Norm groups

DACAD takes the norm from the registry (`HKCU\Software\COIN\DACAD\Default`,
`NormCountry`: 49 DIN 18338, 43 ÖNORM B 2219, 41 SIA 124, 0 or anything else
none), not from the project. With a norm set it adds a "-Norm" group per
line and area group, but stores it only when it differs from the base group
(help: groups identical to an earlier one are suppressed); a missing Norm
group therefore equals the base group. Fixtures:
`tests/fixtures/dacad_norms/{DIN,ONORM,SIA}/` (C04, C24–C31, C37, C38).

Tile area, base → Norm (m²); "=" means no Norm group stored:

| Case | Opening | DIN (≤ 2.5 m²) | ÖNORM (≤ 4 m², windows and chimneys only) | SIA (≤ 1 m²) |
|---|---|---|---|---|
| C24 | window 1.21 m² | 117.194 | 117.194 | = |
| C25 | window 1.12 m² | 117.194 | 117.194 | = |
| C26 | window 0.25 m² | 117.194 | 117.194 | 117.194 |
| C27 | chimney 0.439 m² | 117.194 | 117.194 | 117.194 |
| C28 | chimney across the ridge | 117.194 | 117.194 | 117.194 |
| C29 | gable dormer on the eave (4.667 m² removed) | = | = | = |
| C30 | gable dormer set back (2.057 m² removed) | 127.547 | = | = |
| C31 | shed dormer | = | = | = |
| C37 | window 3.2 m² | = | 117.194 | = |
| C38 | window 4.8 m² | = | = | = |

So an opening at or below the limit is added back in full. DIN also adds back
the host area removed under a small dormer (C30); ÖNORM does not, matching its
text ("Fenster- und Schornstein-Abzugsflächen"). Lines: deductions up to the
limit are ignored in the Norm line groups. C28's ridge is 4.0 in "First-Norm"
under DIN and ÖNORM (0.6 m gap). C29's −2.0 m eave piece is ignored only
under ÖNORM (limit 2 m; "Traufe-Norm" 26.383), not under DIN (limit 1 m).
SIA has no line rule. The ÖNORM "Beideckung" surcharge is not stored in
the file; the help says it is printed in a separate table of the listing.
* Files saved by this DACAD build store the footprint clockwise (wall 2 of
  C08 runs from (12, 8) to (12, 0)); the V1 input ran counter-clockwise.

### The sample projects, recalculated

The results stored in the sample files come from older DACAD versions
(1994–2006). Recalculated by the current DACAD (Wine, all 34 samples), 13
differ from what is stored, e.g. TRAING07 T 49.359 → 47.980 m plus a new
S 1.4 m, HINT04 OL/OR 39.11 → 30.38 m, E010 OS 33.63 → 45.44 m and
K 36.62 → 23.59 m, TRAING06 F 27.89 → 28.29 m, ZWEIPULT area 248.704 →
248.605 m². The current DACAD is the reference, so the sample fixtures are
re-exported from the recalculated files
(`/mnt/project-files/dacad-golden-current/`, not committed: the inputs are
DACAD's sample data).

All 42 single-rule cases reproduce exactly when recalculated.

Rust engine d2a5408 against the six V1 samples (current DACAD): 0 of 6
match. Bisecting TRAING08 with `tools/dacad_variants.py` and
`tools/dacad_linediff.py`:

* Coplanar planes merge. Walls 5 and 7 (north, 31°) have eave heights 3.25
  and 2.53 m and are 1.2 m apart in plan, so their planes coincide; the
  90° wall 6 between them yields only a 1.399 m step (OL at x 5.35, OR at
  x 0.15) and the main hip runs on unbroken (G 9.264 m from (0.15, 14.6) to
  (6.18, 8.57)). The engine instead builds a separate gable there (verge
  7.464 m, valley 6.064 m).
* A chimney sitting on a hip line (TRAING08's 0.5 × 0.5 m at x 4.5–5.0,
  y 7.0–7.5) leaves the hip uncut (G 5.187 m through it) and adds WV
  0.5 + 0.4, WS 0.117 + 0.583 + 0.466, WH 0.1 and a G 0.614 inside the
  chimney outline.
* The 30.2°/30.4° bay walls move ridge points by about 0.01 m against
  DACAD (ridge 4.0 vs 3.99 m, valley 8.343 vs 8.336 m).

## Findings in the Python core (`src/tectora/core`)

1. **Single global pitch.** DACAD defines pitch, eave height and overhang per
   wall; Tectora has one pitch per building (+ an optional back pitch on a
   rectangle gable). Mixed pitches, different eave heights, pitch breaks and
   priorities are not representable.
2. **Half-hip is a different roof.** `calculator._calc_half_hip` builds a
   full-height end triangle from the eave corners with a steeper pitch. DACAD's
   Krüppelwalm keeps a vertical gable up to the Krüppel height and starts the
   hip plane there. Example 12 × 8 m, 45°, eave 3 m, Krüppel from 5.5 m at 60°:
   DACAD gives 137.286 m² (case `C08`, calculated by DACAD), Tectora gives
   158.71 m² for the same ridge clip, or 146.59 m² for the same end pitch.
3. **Overhang is inconsistent.** `RoofSpec.roof_length/roof_width` add the
   overhang on all four sides (gable 10 × 8, 40°, 0.5 m → 129.24 m², ridge
   11.0 m), while `roof_generator.generate_roof` ignores the overhang. DACAD
   applies it horizontally per wall, and a 90° wall's overhang extends the
   verge (`C02` 129.235 m², `C03` 117.487 m²).
4. **Shed top edge counted as eave.** `_calc_shed` returns
   `eaves_perimeter_m = 2·L`; DACAD reports the low edge as T and the top edge
   as P (Pult).
5. **Line classification** has ridge/hip/valley/eave/gable-left/right only;
   DACAD's OS, S, WH/WS/WV, P and Knick have no counterpart, so a take-off by
   DACAD groups cannot be produced.
6. No openings (windows, chimneys) and no norm groups.

## Next steps

* Fixture inputs are taken from the V1 originals (`export --inputs`):
  walls counter-clockwise as authored, with `direction_deg` (zero-length
  dormer side walls are oriented only by it), roof numbers, `windows`
  (x, y, width, height) and `chimneys` (height, plan corners). DACAD's
  re-saved V2 file drops wall directions and dormer roof numbers, and its
  window record is not decoded yet.
* Add cases for each remaining rule (priorities, dormers, windows, chimneys,
  norm deductions) and calculate them with `dacad_wine_calc.sh`.
* Run `compare` against the Rust engine (`crates/tectora-core`) as soon as its
  CLI prints `quantities.lines_by_type` / `area_by_material`.
* If a rule stays ambiguous after the cases (e.g. priority between
  intersecting planes), decompile the calculation in `DACAD DE.exe` with
  Ghidra (rea can drive it).
