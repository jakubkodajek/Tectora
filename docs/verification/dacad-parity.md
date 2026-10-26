# DACAD parity — verification plan and findings

Goal: Tectora's roof quantities must equal DACAD's for the same input.

## Sources of truth

| Source | What it gives | Status |
|---|---|---|
| `morluto/rea` | Generic reverse-engineering MCP toolkit (Ghidra/Hopper bridge). Contains **no** DACAD formulas; usable only as a tool to decompile `DACAD DE.exe`. | not needed so far |
| `DACAD.HLP` (German WinHelp, 2003) | Input model, line types, rounding, norm rules. Decoded with a WinHelp/Hall-phrase extractor. | read |
| `Beispiel/*.DAD` (34 sample projects, 1994–98) | Input **and** DACAD-computed results (every line with type and length, every face area, group totals). | decoded, see below |
| `tools/dacad_testcases.py` | 14 single-rule projects to be calculated and saved in DACAD. | generated, waiting for a DACAD run |

## DACAD's input model (from the help file)

* Every footprint wall carries its own roof definition: pitch, eave height
  (Traufenhöhe), eave overhang (Traufenvorsprung, **measured horizontally**),
  material (tiles / sheet / none), priority, roof number.
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
```

Tolerance: 0.006 m / 0.006 m² (half a unit of DACAD's 2-decimal output plus
float32 noise in the stored totals).

## Findings in the Python core (`src/tectora/core`)

1. **Single global pitch.** DACAD defines pitch, eave height and overhang per
   wall; Tectora has one pitch per building (+ an optional back pitch on a
   rectangle gable). Mixed pitches, different eave heights, pitch breaks and
   priorities are not representable.
2. **Half-hip is a different roof.** `calculator._calc_half_hip` builds a
   full-height end triangle from the eave corners with a steeper pitch. DACAD's
   Krüppelwalm keeps a vertical gable up to the Krüppel height and starts the
   hip plane there. Example 12 × 8 m, 45°, eave 3 m, Krüppel from 5.5 m at 60°:
   the Krüppelwalm geometry gives 137.29 m² (own derivation of DACAD's model,
   not yet a DACAD output; case `C08`), Tectora gives 158.71 m² for the same
   ridge clip, or 146.59 m² for the same end pitch.
3. **Overhang is inconsistent.** `RoofSpec.roof_length/roof_width` add the
   overhang on all four sides (gable 10 × 8, 40°, 0.5 m → 129.24 m², ridge
   11.0 m), while `roof_generator.generate_roof` ignores the overhang. DACAD
   applies it horizontally per wall; whether a 90° wall's overhang extends the
   verge is what cases `C02`/`C03` will show.
4. **Shed top edge counted as eave.** `_calc_shed` returns
   `eaves_perimeter_m = 2·L`; DACAD reports the low edge as T and the top edge
   as P (Pult).
5. **Line classification** has ridge/hip/valley/eave/gable-left/right only;
   DACAD's OS, S, WH/WS/WV, P and Knick have no counterpart, so a take-off by
   DACAD groups cannot be produced.
6. No openings (windows, chimneys) and no norm groups.

## Next steps

* Run the 14 single-rule cases in DACAD, save, export fixtures, commit them
  (inputs are ours, results are DACAD's numbers).
* Run `compare` against the Rust engine (`crates/tectora-core`) as soon as its
  CLI prints `quantities.lines_by_type` / `area_by_material`.
* If a rule stays ambiguous after the cases (e.g. priority between
  intersecting planes), decompile the calculation in `DACAD DE.exe` with
  Ghidra (rea can drive it).
