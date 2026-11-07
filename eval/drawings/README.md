# Drawing-to-roof evaluation set

Eleven real TERRAN jobs: the technical drawings an AI gets, and the offers the
estimators made from them. The offers are the labels: there is no reference
roof for these jobs, so the score says how close a roof built from the
drawings comes to what TERRAN priced.

The drawings and offers themselves are customer data and are **not in the
repository**. Only numbers are here:

- `truth.json`: per job (`"1"` … `"11"`), the number of drawing PDFs, every
  offer's covering, pitch groups, roof area, piece counts of line materials
  and the line-length ranges those counts allow, and for job 6 the exact
  line lengths of a Bramac offer. Made by `tools/drawing-eval/extract_truth.py`
  for jobs 1–10. Job 11 was added by hand (its `note` says what): the offer
  as the script reads it, `also_deg` on its pitch group because the offer
  prices a 26° wing inside its 30° area, and the ridge of the job's bill of
  quantities. Add it again after regenerating the file.
- `drawing_facts.json`: figures printed on some drawings (scale, stated
  areas, heights), read by eye. Cross-checks, not offer figures.
- `public_truth.json`: public roofs from Czech public tenders (`P1`, `P2`,
  …) in the same shape: the bill of quantities or the totals printed on
  the roof plan, with the tender page each comes from. The tender files
  are not stored here; download them again from `source`. A bill marked
  `gross` counts more than the drawn roof (its `note` says how), and one
  marked `unexplained` differs from the drawn roof for a reason its files
  do not show; their areas are printed as information, not scored. Exact
  lines are `ridge_and_hip`, or `ridge` and `hip` where the bill splits
  them, `valley`, `eave` and `verge_left` + `verge_right`.

## How exact each label is

| label | source | exact? |
| --- | --- | --- |
| roof area per pitch group | offer, "plocha střechy" | each group rounded up to a whole m², so the true value is in (A − 1, A] |
| F+G (ridge + hip) | ridge tile count, 2.8 per m, packs of 4 | range of about 1.4 m; rule checked on job 6 only (134.82 m → 378 → 380 pieces) |
| OL+OR (verges) | verge tiles, 3 per m | ±5 %, reserve unknown |
| T (eave) | eave flashing, 2 m pieces | 85–100 % of pieces × 2 m, reserve unknown |
| K (valley) | valley strips, 2 m pieces | 80–100 % of pieces × 2 m, reserve unknown |
| job 6 line lengths | Bramac "Technická data" | exact to 0.01 m (checked at ±1 %) |

Ridge tile counts include end and split pieces in some offers, so F+G can
miss by a few percent on roofs with many ridge ends; read a miss there
together with the drawing before calling the roof wrong.

## Running it

```sh
# labels from the job folder (needs xlrd and openpyxl)
python3 tools/drawing-eval/extract_truth.py "<folder with 1/ … 10/>" -o eval/drawings/truth.json

# score designs built from the drawings
cargo build -q -p tectora-cli
python3 tools/drawing-eval/evaluate.py eval/drawings/truth.json \
  --designs designs.json --library <jobs folder> --tectora target/debug/tectora
```

`designs.json` maps a job to a `.tectora` file or a library job id:
`{"10": {"id": "<job id>"}, "7": "/path/roof.tectora"}`. Job 4 has two
buildings with one offer each: `"4A": {"id": "…", "offer": 0}`,
`"4B": {"id": "…", "offer": 1}`. The script prints every check and exits
with 1 if any fails; `--json out.json` writes the report.
