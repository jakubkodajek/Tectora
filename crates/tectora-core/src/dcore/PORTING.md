# dcore: porting DACAD's roof core

`dcore` is a 1:1 port of DACAD's roof calculation core: RD2 in, core SOL (`.SO2`) and INT out.
It is checked against an emulator that runs the original code. The behavioural map is
`/tmp/claude-0/re/spec/core/map.md` (pipeline, records, slices S0–S7). This file covers what the
map does not: how the Rust side is organised, the arithmetic rules, and how to test a slice
against the emulator.

## What exists

| module | content | slice |
|---|---|---|
| `num` | tolerances (`Tol`), compares `eq ne lt gt le ge zero peq pne vzero`, vector ops, `len`, `dot`, `norm`, `angle`, `par`, `orient`, `solve2`/`solve3` (Cramer), `rise` | S0 |
| `x87` | `X80` (an x87 register value) with its mixed ops `fmul fdiv fdivr fadd fsub`, `fsin fcos fptan fpatan crt_acos` at 64-bit mantissa | S0 |
| `geom` | `cross`, `horizontal`, `coplanar`, `plane_plane`, `line_plane`, `line_line`, plus `Line`, `Plane` and `NO_POINT` | S0 |
| `state` | arenas and typed ids for every record kind, `Globals`, the code table, `ctr` counter indices, constructors | data model |
| `rd2` | the RD2 reader (fscanf semantics, list prepends, cursor globals) | S1 |
| `s1` | ring sort and collinear merge, hole relink, frames and planes, eave faces, validity ranges, eave heights, levels (`model_start`) | S1 |
| `stages` | **stubs** for S2, S3, S5, S6 (S4 calls `post`), one per routine of the driver | S2–S6 |
| `post` | collinear merge, segment origins and edge table, face areas, dormer feet | S4 |
| `faceops` | segment insertion `add_seg`, `trim_crossings`, `seg_cross`, `ends_at`, the closing test `close_face`, `drop_sentinel` (non-wall faces; used by the dormer pass) | S3 (part) |
| `pipeline` | the driver (`Run`): per-model loop, global stages, writers, hooks | — |
| `out` | core SOL, INT, fallback SOL, `PLACEHOLDER_SOL`; they fill `coresol::CoreSol` / `IntGeometry` | S7 |
| `dump` | the emulator's `state.json` format: `to_json`, `from_json`, structural `compare` | testing |
| `error` | `CoreError` and the message table | — |

## Validation status

The ignored test `emulator_dumps` was run on the local dump sets. All of these are equal:

| set | cases | what is equal |
|---|---|---|
| 34 sample captures (`list_caps`) | 34 | after_read, s1_ring/frames/end, SO2 and INT bytes and state from the emulator's final state |
| dacad-core (`list_core`) | 107 | as above (SO2 only; 106 reach the writers, C68 ends with 0xa0f) |
| dacad-mass s1 (`list_s1`) | 615 of 616 | as above. R1_0340 never returns: the emulator loops inside the sweep's line∩line (`d98600`), as DACAD does |
| models after the first (`list_multi`) | 133 | every model's S1 run from the emulator's `s1_entry` state (405 model runs) |
| synthetic error RD2s | 8 | outcome: RD2 errors 0x9fe/0x9ff/0xf120/"L error", the interval error, and "Kein geschlossener Grundriss" with its fallback SOL |

The other checks:

- SO2 bytes: 581 equal. INT bytes: 34 equal.
- x87 transcendentals: 6523 emulator vectors, all equal.
- S0 primitives: 44000 random emulator vectors, all equal.

`cargo test -p tectora-core --test dcore_emulator` runs the committed part:

- 10 fixture cases with full-stage dumps (`tests/fixtures/dcore/*/state.json.gz`);
- 1650 S0 vectors (`geom.tsv`);
- 50 x87 vectors (`x87.tsv`).

## Arithmetic rules (S0)

- Every record field and every `float` local is `f32`. Between two stores the x87 runs at 53-bit
  precision, which is exactly `f64` arithmetic on the widened values.
- Write each expression in the order the disassembly evaluates it:
  - `fsubp`/`fdivp` operand order matters;
  - `fmul m32` multiplies the stack top;
  - `(a*b)*c` is not `a*(b*c)`.
- Widen with `f64::from`. Round with `as f32` exactly where the original stores to a float, and
  wherever it pushes a float argument: every helper of map §0 takes `f32` arguments.
- `num::len` stays unrounded (`f64`) until a caller stores it.
- Transcendentals: `x87::fcos(x)` returns an `X80` (64-bit mantissa). Its next operation rounds
  once: `fcos(a).fmul(len)`, `fptan(p).fdivr(dh)`, `.to_f32()` for a direct `fstp dword`. Do not
  go through `to_f64()` first unless the original stores a double. `angle` uses the CRT acos.
- The compares follow the x87 flag logic, NaN included (`lt(NaN, x)` is true). Use the `num`
  helpers; never `<` on floats where the original calls a helper.
- Read the disassembly (`dacdis.py FUN_xxx`), not just the pseudo-C. Ghidra drops `this` and stack
  arguments, mislabels stack slots, and hides the order of x87 operations; `geom::line_line` is an
  example where the pseudo-C names the wrong solution component.
- Never use a binary address in Rust code. Name things after what they hold.

## State model

- `Core` holds one `Vec` per record kind. Records refer to each other by typed ids (`FaceId`,
  `Option<SegId>` for a nullable pointer).
- Records are never removed. Where the original unlinks or frees a record, only unlink it, so ids
  stay valid. The comparer only looks at what is reachable from the globals and the static faces,
  so freed records do not matter.
- Faces 0–2 are the static faces `SFACE1`, `SFACE2` and `TFACE`. Every wall owns an embedded eave
  face (`Wall::eave`; in the original it is part of the wall record).
- List order is state, so keep the original's discipline:
  - models are prepended;
  - holes and their segment lists are prepended, then relinked once per model;
  - faces are appended per wall;
  - CUTs are appended (`g.cut_tail`);
  - EDGEs are prepended;
  - levels are a sorted insert.
- `g.counters` is the 18-word counter block of the current model. Its indices are in `state::ctr`.
  It is saved to and restored from `Model::counters` where the original copies it.
- Field names are the source of truth shared with the dumps. The offset → name table is `LAYOUT`
  in `dump_state.py`, which matches the structs in `state.rs`.
- To add a field (for example a global the sweep uses):
  1. add it to `state.rs` (and its `Default`);
  2. add it to `dump::to_json` and `dump::from_json`;
  3. add it to `LAYOUT` (records) or the globals list in `dump_state.py`;
  4. regenerate the dumps you test with.

  Unknown fields keep a neutral name (`x14`, `scratch100`) until their meaning is clear.

## The pipeline and the stubs

`pipeline::Run::run` follows the driver (map §1.1).

**Per model** (list order):

1. Reset, load the counters and the ring. A `'w'` model without flag 0x20 is skipped and sets
   `ok`.
2. `plane_cut`: first `s1::model_start` (hooks `s1_ring`, `s1_frames`, `s1_end`), then
   **`stages::sweep`** (hook `after_cut`).
3. **`chimney_holes`** (flag 0x8 only; hook `after_holes` either way).
4. **`collinear_merge`** (only if `ok`; its count is subtracted from `counters[ctr::LINES]`; hook
   `after_merge`).
5. **`edge_table`** (`after_edgetab`).
6. Break if `!ok`.
7. **`areas`** (`after_area`). `n_g_models += 1` for a `'G'` model.

**After the loop:** `!ok` ends the run with `NoSolution` (progress 0xa10, which DACAD cancels).
Otherwise the global stages run, each only while `ok` stays set. They are listed below with the
hook each one fires:

| stage | hook |
|---|---|
| **`dormer_feet`** | `after_dormer` |
| **`classify`** | `after_classify` |
| **`codes`** | `after_codes1` |
| **`merge_d17`** | `after_merge17` |
| **`merge_lg17`** | `after_mergeLG` |
| **`hole_edges`** | `after_holeedge` |
| **`codes`** (again) | `after_codes2` |

Then `write_sol` (`after_sol`) and `write_int` (`after_int`).

**Errors:**

- A cut error from the sweep is `CoreError::Cut { code: 2, msg }`. The model's remaining stages
  still run for the state; a stub that returns `Unimplemented` does not hide the cut error.
- A broken wall chain (`NoPlan`) returns `Ok` with code 3 and the fallback SOL.
- Fatal errors (`Fatal { code: 1 | 2, .. }`) leave `out::PLACEHOLDER_SOL` behind in DACAD.

**To port a stage:** replace its stub body, or move it into your own module and call it from the
stub. Keep the signature and the hook point. The hook names are the emulator's snapshot names, so
the end-to-end test picks up your stage automatically.

## Testing against the emulator

Tools (not in the repo; they drive the emulator of DE.exe) are in
`/tmp/claude-0/re/spec/core/dcore/`. Working copies and the dump sets are in `/tmp/claude-0/coreA/`.

```
# one case, all stages (add --sweep for a snapshot at every progress call inside the plane cut)
python3 dump_state.py IN.RD2 OUTDIR [--only=s1_end,after_cut,...] [--int|--noint] [--x87=FILE]
# a list (lines "path.RD2[<TAB>int|noint]"), in parallel; skips cases already dumped
ONLY=s1_end,after_cut,after_codes2 bash run_dumps.sh LIST OUTROOT 3
# the Rust side over dump sets
DCORE_SETS="list_core.txt=dumps/core;list_s1.txt=dumps/s1" \
    cargo test --release -p tectora-core --test dcore_emulator emulator_dumps -- --ignored --nocapture
# S0 vectors: random inputs through the original routines
python3 geom_vectors.py SEED N OUT.tsv
DCORE_GEOM=OUT.tsv cargo test -p tectora-core --test dcore_emulator geom_all_vectors -- --ignored
```

**Snapshot names.** `s1_entry` is the state at the entry of each per-model plane cut. A
multi-model case has one per model, told apart by `call`. `sweep:<msgid>` snapshots appear with
`--sweep`, one per phase and level.

**Testing a slice in isolation.** Start from the emulator's state before your stage, run only
your stage, and compare with the emulator's state after it:

```rust
let doc = dump::DumpDoc::load(path)?;
let mut c = dump::from_json(doc.stage("s1_end")[k])?;   // k-th model
stages::sweep(&mut c)?;
let diffs = dump::compare(doc.stage("after_cut")[k], &dump::to_json(&c), 20);
```

`compare` labels records by their first visit from the globals, so allocation numbers do not have
to match. Only the order of reachable links does. The report names the path that reached a
differing record, for example `FACE@12 (g.ring.faces.next).zhi`.

**Fixtures.** Commit only our own cases: C-cases from `/mnt/project-files/dacad-core/`, C67, and
synthetic RD2s. Never commit DACAD sample projects, `.DAD` files or decompiled code. A fixture is
`tests/fixtures/dcore/NAME/{NAME.RD2, state.json.gz, NAME.SO2[, NAME.INT]}`. Dump it with all
stages and `gzip -9` the state. The `fixtures` test then checks everything the port reaches. The
current fixtures cover these cases:

- C01 gable;
- C24 window;
- C27 chimney (flag 0x8 holes);
- C29 dormer ('g' model);
- C46 two roofs with INT;
- C67 two models with INT;
- C68 cut error 0xa0f;
- S01 broken chain;
- S02 bad wall record;
- S07 interval error.

## Corrections to map.md found while porting

- `orient` (`d981b0`) returns **+1** when the value is clearly negative (`lt(v, 0)`), else −1;
  0 within VALUE_EPS. The map has the signs swapped.
- `coplanar` (`d982b0`) compares the offsets only through the x components (`a.d·b.n.x` against
  `b.d·a.n.x`). Parallel planes with `n.x = 0` are therefore "coplanar" whatever their offsets.
- `line_line` (`d98600`) checks the point against the second line in all three coordinates. It
  moves on to the next coordinate pair (xy, xz, yz) both when a solve fails and when the check
  fails. The output keeps the last point computed, even when the result is 0.
- The hole relink guard (an uninitialised local) was never 1 in any dump. The `relink guard` tally
  of the test watches for it.

## Notes per slice

**S2/S3: sweep, segments, closing.** Start from `s1_end` and compare at `after_cut`. Use `--sweep`
snapshots for per-level checks.

- The S0 line and plane routines are ready in `geom`. Still to port, in `geom` with vectors from
  `geom_vectors.py`: `d98830` (seg/seg with mode bits), `d99110`, `d99460`, `d99590`, `d999b0`,
  `d9a080`, `da0f90`, `e561f0`, `e56000`.
- `out::close_face` is only the trivial outcome of `da20f0`. The real closing belongs to S3; the
  INT writer calls it on the ground faces.
- The segment counter `counters[ctr::SEGS]` ends the sweep (map §7.4).
- R1_0340 does not terminate in the original. A port that loops there is faithful; give it a
  guard in tests.
- `'w'` models return `Unimplemented` in `s1::model_start` (`d9cc10`); no sample has one.

**S4: merge, edge table, areas, dormer feet.**

- Hook points: `after_merge`, `after_edgetab`, `after_area`, `after_dormer`. Ported in `post`;
  the test `s4_slices` runs each stage of each model from the emulator's state before it
  (`after_holes` → merge, `after_merge` (or `after_holes` for a failed model) → edge table,
  `after_edgetab` → areas, the last `after_area` → dormer feet). Dump sets for it need only
  `--only=after_holes,after_merge,after_edgetab,after_area,after_dormer`; run the test with
  `DCORE_REPORT=s4` to leave out the tallies of stages such dumps lack.
- The model loop variable is the current-model global: it is null after the last model (the
  pipeline sets it), which `after_dormer` shows.
- Merge: the absorbed segment's successor keeps its stale `prev`; the last segment joining the
  first is cut off by `prev.next = null` without moving the list head.
- Edge table: the representative swap and the two face-exchange blocks use the values read from
  the representative *before* the swap (faces, kind); `same_line` and the direction use
  POINT_EPS. Hole pieces join only 'L'/'G' edges.
- Areas: the angles to the y and x axes are `acos(f32(n.y or n.x) / |n|)` (the dot is stored, the
  axis length is not used), the pitch is the face field; each is folded to `pi - a` when clearly
  above pi/2 (VALUE_EPS); the smallest picks the projection (ties: pitch, then x, then y). An
  all-vertical normal raises fatal 2 with 0xa12 "Flächeninhalt konnte nicht berechnet werden".
  The three `fcos` there are in section EXTRA, which the stock emulator does not route to the
  x87 model; see the validation note below.
- Dormer feet call the S3 routines `da1210` (segment insertion) and `da20f0` (closing) on the
  model's 'G' face. `faceops` ports the paths a non-wall face takes, including `da1c40`
  (trimming at crossings, `d98830` seg/seg in all modes but only mode 0 checked) and `e56170`.
  Two quirks are kept: `close_face` leaves POINT_EPS grown when a retry closes the face, and
  `drop_sentinel` writes the list head of `g.cur_face` (the sweep's face) when it unlinks a first
  segment. The hidden wall face branch of `da1210` returns `Unimplemented`.
- `geom::collinear_join` (`d99110`): segments sharing their *start* points never join (the
  offset between the starts is the zero vector, which `par` rejects).
- `out::write_sol` checks that the edge list is in descending line order and as long as
  `g.n_lines`.

**S5: classification.** Hook points: `after_classify` to `after_codes2`. The writers already
produce the bytes from the emulator's `after_codes2` state, so once S5 matches the state, the SO2
matches.

**S6: holes.**

- `chimney_holes` runs only with flag 0x8; the default flags 0x4a8 include it.
- `hole_edges` runs after the D/D and L/G code-17 stages.
- The clipper needs VC6 `qsort` (map §7.3).

**Writers (S7).**

- `write_sol` without flag 0x20 is not ported (DACAD always passes 0x20).
- The INT ground record of a `D` model is not ported (never reached in the sets).
