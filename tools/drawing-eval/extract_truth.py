#!/usr/bin/env python3
"""Build the drawing-to-roof evaluation labels from real jobs.

Input: a folder with one sub-folder per job (``1``, ``2``, ...), each holding
the job's technical drawings (PDF) and the offers made from them:

- ``CN *.xls``: TERRAN / Mediterran price offers. They list the roof pitch and
  the roof area per pitch group, and piece counts of the line materials
  (ridge tiles, verge tiles, eave flashing, valley strips) from which the line
  lengths follow only approximately.
- ``*BRAMAC*.xlsx``: Bramac offers with a "Technická data" sheet that lists
  the exact line lengths and every roof face.

Output: ``truth.json`` with numbers only (no customer data), see
``eval/drawings/README.md`` for the format and how exact each label is.

Usage: python3 extract_truth.py "<folder>" -o eval/drawings/truth.json
Needs: xlrd (for .xls) and openpyxl (for .xlsx).
"""

from __future__ import annotations

import argparse
import json
import math
import re
import sys
import unicodedata
from pathlib import Path
from typing import Any

# Piece rates printed in the offers' item names ("Hřebenáč (2,8 ks/bm)", ...).
RIDGE_TILES_PER_M = 2.8
RIDGE_TILE_PACK = 4  # every ridge tile count in the offers is a multiple of 4
VERGE_TILES_PER_M = 3.0
EAVE_FLASHING_M = 2.0
VALLEY_STRIP_M = 2.0


def nfc(s: str) -> str:
    return unicodedata.normalize("NFC", s)


def num(v: Any) -> float | None:
    if isinstance(v, (int, float)) and math.isfinite(v):
        return float(v)
    return None


def read_cn(path: Path) -> dict[str, Any]:
    """Reads the figures of one TERRAN / Mediterran offer (sheet "CP SK")."""
    import xlrd  # type: ignore[import-untyped]

    sheet = xlrd.open_workbook(str(path)).sheet_by_index(0)

    def row_by_label(prefix: str, col: int) -> list[Any] | None:
        for r in range(min(200, sheet.nrows)):
            v = sheet.cell_value(r, col)
            if isinstance(v, str) and nfc(v).strip().startswith(prefix):
                return [sheet.cell_value(r, c) for c in range(sheet.ncols)]
        return None

    def qty(prefix: str) -> float:
        row = row_by_label(prefix, 1)
        return (num(row[2]) or 0.0) if row else 0.0

    pitch_row = row_by_label("sklon", 3) or []
    area_row = row_by_label("plocha střechy", 3) or []
    pitches = [p for p in (num(pitch_row[4]), num(pitch_row[5])) if p]
    areas = [a for a in (num(area_row[4]), num(area_row[5])) if a]
    covering = row_by_label("Druh krytiny", 1)
    return {
        "covering": nfc(str(covering[1])).split(":", 1)[-1].strip() if covering else None,
        "pitch_groups": [
            {"pitch_deg": p, "area_m2": a} for p, a in zip(pitches, areas)
        ],
        "roof_area_m2": sum(areas),
        "pieces": {
            "ridge_tiles": qty("Hřebenáč ("),
            "ridge_end_tiles": qty("Koncový hřebenáč"),
            "ridge_split_y": qty("Rozdělovací hřebenáč Y"),
            "ridge_split_x": qty("Rozdělovací hřebenáč X"),
            "verge_tiles_right": qty("Krajní taška pravá"),
            "verge_tiles_left": qty("Krajní taška levá"),
            "eave_flashing_2m": qty("Okapový plech –"),
            "valley_strip_2m": qty("Pás úžlabí"),
            "chimney_flashing": qty("Těsnící pás kolem komínu"),
            "roof_exit_window": qty("Střešní okno výstupní"),
        },
    }


def read_bramac(path: Path) -> dict[str, Any]:
    """Reads the "Technická data" sheet of a Bramac offer."""
    import openpyxl  # type: ignore[import-untyped]

    ws = openpyxl.load_workbook(str(path), data_only=True)["Technická data"]

    def metres(v: Any) -> float | None:
        m = re.match(r"\s*([0-9]+(?:[.,][0-9]+)?)", str(v or ""))
        return float(m.group(1).replace(",", ".")) if m else None

    lines: dict[str, float] = {}
    faces: list[dict[str, Any]] = []
    names = {
        "Hřeben": "ridge_and_hip",
        "Úžlabí": "valley",
        "Okapní hrana": "eave",
        "Štítová hrana levá": "verge_left",
        "Štítová hrana pravá": "verge_right",
        "Boční napojení na svislou kci.": "abutment_side",
        "Přední napojení na svislou kci.": "abutment_front",
    }
    for row in ws.iter_rows(values_only=True):
        label = nfc(str(row[0] or "")).strip()
        if label in names:
            value = next((metres(v) for v in row[1:] if metres(v) is not None), None)
            if value is not None:
                lines[names[label]] = value
        elif re.fullmatch(r"Z\d+\.\d+", label):
            faces.append({
                "name": label,
                "pitch_deg": metres(row[1]),
                "area_m2": metres(row[2]),
            })
    by_pitch: dict[float, float] = {}
    for f in faces:
        if f["pitch_deg"] is not None and f["area_m2"] is not None:
            by_pitch[f["pitch_deg"]] = by_pitch.get(f["pitch_deg"], 0.0) + f["area_m2"]
    return {
        "lines_m": lines,
        "faces": len(faces),
        "area_by_pitch_m2": [
            {"pitch_deg": p, "area_m2": round(a, 2)} for p, a in sorted(by_pitch.items())
        ],
        "roof_area_m2": round(sum(f["area_m2"] or 0.0 for f in faces), 2),
    }


def line_intervals(pieces: dict[str, float]) -> dict[str, list[float]]:
    """Ranges of line lengths (m) that the offer's piece counts allow.

    Ridge tiles: count = ceil(L * 2.8) rounded up to a pack of 4, checked on
    job 6 against Bramac's exact lengths (134.82 m -> 378 -> 380). Verge
    tiles, eave flashing and valley strips carry reserves the offers do not
    state; their ranges are wide on purpose (job 6: verge 236 pcs for
    79.40 m, eave 98 x 2 m for 186.78 m, valley 68 x 2 m for 120.62 m).
    """
    out: dict[str, list[float]] = {}
    rt = pieces.get("ridge_tiles", 0.0)
    if rt > 0:
        out["F+G"] = [round(max(rt - RIDGE_TILE_PACK, 0) / RIDGE_TILES_PER_M, 2),
                      round(rt / RIDGE_TILES_PER_M, 2)]
    vl, vr = pieces.get("verge_tiles_left", 0.0), pieces.get("verge_tiles_right", 0.0)
    if vl + vr > 0:
        mid = (vl + vr) / VERGE_TILES_PER_M
        out["OL+OR"] = [round(mid * 0.95, 2), round(mid * 1.05, 2)]
    ef = pieces.get("eave_flashing_2m", 0.0)
    if ef > 0:
        out["T"] = [round(ef * EAVE_FLASHING_M * 0.85, 2), round(ef * EAVE_FLASHING_M, 2)]
    vs = pieces.get("valley_strip_2m", 0.0)
    if vs > 0:
        out["K"] = [round(vs * VALLEY_STRIP_M * 0.8, 2), round(vs * VALLEY_STRIP_M, 2)]
    return out


def pdf_count(folder: Path) -> int:
    # Only the count: file names carry customer names and places.
    return sum(1 for _ in folder.glob("*.pdf"))


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("folder", type=Path)
    ap.add_argument("-o", "--output", type=Path, required=True)
    args = ap.parse_args()

    jobs = []
    for d in sorted((p for p in args.folder.iterdir() if p.is_dir()), key=lambda p: (len(p.name), p.name)):
        offers = [read_cn(p) for p in sorted(d.glob("CN*.xls"))]
        bramac = [read_bramac(p) for p in sorted(d.glob("*.xlsx")) if "BRAMAC" in p.name.upper()]
        for o in offers:
            o["line_intervals_m"] = line_intervals(o["pieces"])
        jobs.append({
            "job": nfc(d.name),
            "drawings": pdf_count(d),
            "offers": offers,
            "exact": bramac,
        })
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps({"version": 1, "jobs": jobs}, ensure_ascii=False, indent=1) + "\n")
    print(f"{len(jobs)} jobs, {sum(len(j['offers']) for j in jobs)} offers, "
          f"{sum(len(j['exact']) for j in jobs)} exact -> {args.output}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
