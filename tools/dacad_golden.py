"""Golden corpus from DACAD's own sample projects, and a comparator.

Every DACAD ``.DAD`` file stores the input and the results DACAD computed for
it, so each one is a reference case for "same numbers as DACAD".

    # 1) Extract fixtures (inputs + DACAD's stored results) to JSON:
    python tools/dacad_golden.py export /path/to/DACAD/Beispiel out/golden

    # 2) Compare an engine against them. The engine command gets the .DAD path
    #    and must print JSON with "lines_by_type" {DACAD code: metres} and
    #    "area_by_material" {"Ziegel": m², ...} (top level or under
    #    "quantities"):
    python tools/dacad_golden.py compare /path/to/DACAD/Beispiel \\
        --engine "cargo run -q -p tectora-cli -- calc {file}"

The .DAD files themselves are proprietary sample data and are not committed;
point the tool at a local DACAD installation.
"""
from __future__ import annotations

import argparse
import json
import shlex
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from tectora.io.dacad_dad import LINE_TYPES, DadProject, read_dad  # noqa: E402

# DACAD rounds displayed lengths and areas to 2 decimals (help: Einheiten →
# "Genauigkeit Längen/Flächen", default 2). Allow half a unit in the last place
# plus float32 noise on the stored totals.
TOL_LEN_M = 0.006
TOL_AREA_M2 = 0.006

# Line types that matter for a roofer's take-off (Wandlinie etc. excluded).
TAKEOFF_TYPES = [LINE_TYPES[c] for c in sorted(LINE_TYPES)]

# Coarse codes an engine may print instead of DACAD's split types.
AGGREGATES = {"O": ["OL", "OR", "OS"], "W": ["WH", "WS", "WV"]}


def fixture(pr: DadProject) -> dict:
    roofs = sorted({d.roof_no for w in pr.walls for d in w.definitions})
    return {
        "source": Path(pr.path).name,
        "format": pr.version,
        "input": {
            "walls": [
                {
                    "start": [round(c, 4) for c in w.start],
                    "end": [round(c, 4) for c in w.end],
                    "definitions": [
                        {
                            "pitch_deg": round(d.pitch_deg, 4),
                            "eave_height_m": round(d.eave_height_m, 4),
                            "overhang_m": round(d.overhang_m, 4),
                            "material": d.material,
                            "roof_no": d.roof_no,
                        }
                        for d in w.definitions
                    ],
                }
                for w in pr.walls
            ],
        },
        "traits": {
            "roof_numbers": roofs,
            "stacked_definitions": any(len(w.definitions) > 1 for w in pr.walls),
            "pitches": sorted({round(d.pitch_deg, 2) for w in pr.walls
                               for d in w.definitions}),
            "eave_heights": sorted({round(d.eave_height_m, 3) for w in pr.walls
                                    for d in w.definitions}),
            "overhangs": sorted({round(d.overhang_m, 3) for w in pr.walls
                                 for d in w.definitions}),
        },
        "dacad_results": {
            "lines_by_type": {k: round(v, 4) for k, v in
                              sorted(pr.line_totals().items())},
            "line_groups": {k: round(v, 4) for k, v in pr.line_groups.items()},
            "area_groups": {k: round(v, 4) for k, v in pr.area_groups.items()},
            "line_count": len(pr.lines),
            "face_count": len(pr.face_areas),
        },
    }


def _dad_files(folder: Path) -> list[Path]:
    return sorted(p for p in folder.iterdir() if p.suffix.lower() == ".dad")


def cmd_export(src: Path, out: Path) -> int:
    out.mkdir(parents=True, exist_ok=True)
    for p in _dad_files(src):
        fx = fixture(read_dad(p))
        (out / (p.stem + ".json")).write_text(
            json.dumps(fx, indent=2, ensure_ascii=False), encoding="utf-8")
        print(f"{p.name}: {len(fx['input']['walls'])} walls")
    return 0


def _engine_quantities(cmd: str, path: Path) -> dict:
    argv = [a.replace("{file}", str(path)) for a in shlex.split(cmd)]
    res = subprocess.run(argv, capture_output=True, text=True, timeout=120)
    if res.returncode != 0:
        raise RuntimeError(res.stderr.strip()[-500:] or f"exit {res.returncode}")
    data = json.loads(res.stdout)
    return data.get("quantities", data)


def compare(pr: DadProject, got: dict) -> list[str]:
    """Return human-readable discrepancies (empty list = exact match)."""
    issues: list[str] = []
    want_lines = pr.line_totals()
    got_lines = got.get("lines_by_type", {})
    types = list(TAKEOFF_TYPES)
    # Engines may report coarser groups until they split them by side/kind.
    for agg, parts in AGGREGATES.items():
        if agg in got_lines:
            want_lines[agg] = sum(want_lines.pop(p, 0.0) for p in parts)
            types = [t for t in types if t not in parts] + [agg]
    for t in types:
        w, g = want_lines.get(t, 0.0), float(got_lines.get(t, 0.0))
        if abs(w - g) > TOL_LEN_M:
            issues.append(f"line {t}: DACAD {w:.3f} m, engine {g:.3f} m "
                          f"(Δ {g - w:+.3f})")
    got_areas = dict(got.get("area_by_material", {}))
    if "roof" in got_areas and "Ziegel" not in got_areas:
        # Engine without materials yet: compare its total to tiles + sheet.
        if "Ziegel" in pr.area_groups or "Blech" in pr.area_groups:
            w = pr.area_groups.get("Ziegel", 0.0) + pr.area_groups.get("Blech", 0.0)
        else:  # DACAD 1.13 only stores "Flächen gesamt" (all roof faces)
            w = pr.area_groups.get("Flächen gesamt", 0.0)
        g = float(got_areas["roof"])
        if abs(w - g) > TOL_AREA_M2:
            issues.append(f"area roof: DACAD {w:.3f} m², engine {g:.3f} m² "
                          f"(Δ {g - w:+.3f})")
    for name in [n for n in ("Ziegel", "Blech") if n in got_areas]:
        if name in pr.area_groups:
            w, g = pr.area_groups[name], float(got_areas.get(name, 0.0))
            if abs(w - g) > TOL_AREA_M2:
                issues.append(f"area {name}: DACAD {w:.3f} m², engine {g:.3f} m² "
                              f"(Δ {g - w:+.3f})")
    return issues


def cmd_compare(src: Path, engine: str) -> int:
    passed = failed = errored = 0
    for p in _dad_files(src):
        pr = read_dad(p)
        try:
            got = _engine_quantities(engine, p)
        except Exception as exc:  # noqa: BLE001 — report and continue
            errored += 1
            print(f"ERROR {p.name}: {exc}")
            continue
        issues = compare(pr, got)
        if issues:
            failed += 1
            print(f"FAIL  {p.name}")
            for line in issues:
                print(f"      {line}")
        else:
            passed += 1
            print(f"OK    {p.name}")
    print(f"\n{passed} match, {failed} differ, {errored} errors")
    return 0 if failed == 0 and errored == 0 else 1


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    e = sub.add_parser("export")
    e.add_argument("src", type=Path)
    e.add_argument("out", type=Path)
    c = sub.add_parser("compare")
    c.add_argument("src", type=Path)
    c.add_argument("--engine", required=True)
    a = ap.parse_args(argv)
    if a.cmd == "export":
        return cmd_export(a.src, a.out)
    return cmd_compare(a.src, a.engine)


if __name__ == "__main__":
    raise SystemExit(main())
