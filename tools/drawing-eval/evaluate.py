#!/usr/bin/env python3
"""Score roofs built from drawings against the evaluation labels.

Give it ``truth.json`` (from extract_truth.py) and, per job, the Tectora
design built from that job's drawings: a ``.tectora`` file or a library job
id. It runs ``tectora calc --summary`` on each and compares:

- roof area per pitch group and in total with the offer (the offer rounds
  each group up to whole m², so the true value lies in (A - 1, A]);
- line lengths (F+G, OL+OR, T, K) with the ranges the offer's piece counts
  allow, or with the exact lengths where a Bramac offer has them;

Usage:
  python3 evaluate.py truth.json --designs designs.json [--library DIR] [--tectora BIN] [--json out.json]

``designs.json`` maps job keys to designs: {"7": "/path/valec.tectora",
"4A": {"id": "<library id>", "offer": 0}, ...}. For jobs with two buildings
(job 4) give "4A"/"4B" with the offer index. Jobs not listed are skipped.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path
from typing import Any

# Area tolerance on top of the offer's rounding, relative.
AREA_TOL = 0.01
# Exact lengths (Bramac): relative tolerance.
EXACT_TOL = 0.01


def calc(tectora: str, design: Path) -> dict[str, Any]:
    out = subprocess.run([tectora, "calc", str(design), "--summary"], capture_output=True, text=True)
    data = json.loads(out.stdout or "{}")
    if not data.get("ok"):
        raise RuntimeError(data.get("error", out.stderr.strip() or "calc failed"))
    return data["quantities"]


def library_design(library: Path, job_id: str) -> Path:
    p = library / f"{job_id}.tectora"
    if not p.exists():
        raise FileNotFoundError(f"no design {job_id} in {library}")
    return p


def check_range(value: float, lo: float, hi: float) -> dict[str, Any]:
    ok = lo <= value <= hi
    mid = (lo + hi) / 2
    return {"value": round(value, 2), "range": [lo, hi], "ok": ok,
            "error_pct": round((value - mid) / mid * 100, 2) if mid else None}


def score(q: dict[str, Any], offer: dict[str, Any], exact: dict[str, Any] | None) -> dict[str, Any]:
    lines = q.get("lines_by_type", {})
    by_pitch = {round(p["pitch_deg"], 2): p["area"] for p in q.get("area_by_pitch", [])}
    res: dict[str, Any] = {"checks": {}}
    c = res["checks"]

    total = q.get("roof_area", 0.0)
    want = offer["roof_area_m2"]
    groups = offer["pitch_groups"]
    # Each group is rounded up to a whole m² (a decimal group is exact).
    slack = sum(1.0 for g in groups if float(g["area_m2"]).is_integer())
    c["area_total"] = check_range(total, (want - slack) * (1 - AREA_TOL), want * (1 + AREA_TOL))
    for g in groups:
        # Offers merge close pitches into one group (job 6: 42° and 43° -> 43°).
        got = sum(a for p, a in by_pitch.items() if abs(p - g["pitch_deg"]) <= 1.0)
        lo = (g["area_m2"] - (1 if float(g["area_m2"]).is_integer() else 0)) * (1 - AREA_TOL)
        c[f"area_{g['pitch_deg']:g}deg"] = check_range(got, lo, g["area_m2"] * (1 + AREA_TOL))

    have = {
        "F+G": lines.get("F", 0.0) + lines.get("G", 0.0),
        "OL+OR": lines.get("OL", 0.0) + lines.get("OR", 0.0) + lines.get("OS", 0.0),
        "T": lines.get("T", 0.0),
        "K": lines.get("K", 0.0),
    }
    if exact:
        e = exact["lines_m"]
        want_exact = {
            "F+G": e.get("ridge_and_hip"),
            "OL+OR": (e.get("verge_left") or 0) + (e.get("verge_right") or 0) or None,
            "T": e.get("eave"),
            "K": e.get("valley"),
        }
        for k, v in want_exact.items():
            if v:
                c[f"{k}_exact"] = check_range(have[k], v * (1 - EXACT_TOL), v * (1 + EXACT_TOL))
    else:
        for k, (lo, hi) in offer["line_intervals_m"].items():
            c[k] = check_range(have[k], lo, hi)
    res["passed"] = sum(1 for v in c.values() if v["ok"])
    res["total"] = len(c)
    return res


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("truth", type=Path)
    ap.add_argument("--designs", type=Path, required=True)
    ap.add_argument("--library", type=Path)
    ap.add_argument("--tectora", default="tectora")
    ap.add_argument("--json", type=Path)
    args = ap.parse_args()

    truth = {j["job"]: j for j in json.loads(args.truth.read_text())["jobs"]}
    designs = json.loads(args.designs.read_text())
    report = []
    for key, spec in designs.items():
        if isinstance(spec, str):
            spec = {"path": spec}
        job = truth.get(key.rstrip("ABCDEFGH")) or truth.get(key)
        if job is None:
            print(f"{key}: no such job in truth", file=sys.stderr)
            continue
        offer = job["offers"][spec.get("offer", 0)]
        path = Path(spec["path"]) if "path" in spec else library_design(args.library, spec["id"])
        try:
            r = score(calc(args.tectora, path), offer, job["exact"][0] if job["exact"] else None)
        except Exception as e:  # report and go on with the other jobs
            r = {"error": str(e), "passed": 0, "total": 0, "checks": {}}
        r["job"] = key
        report.append(r)

    width = max([len(k) for r in report for k in r["checks"]] + [10])
    for r in report:
        head = f"job {r['job']}: {r['passed']}/{r['total']} checks"
        print(head + (f"  ERROR {r['error']}" if "error" in r else ""))
        for k, v in r["checks"].items():
            mark = "ok " if v["ok"] else "BAD"
            print(f"  {mark} {k:<{width}} {v['value']:>9.2f}  in [{v['range'][0]:.2f}, {v['range'][1]:.2f}]")
    passed = sum(r["passed"] for r in report)
    total = sum(r["total"] for r in report)
    print(f"\n{len(report)} designs, {passed}/{total} checks passed")
    if args.json:
        args.json.write_text(json.dumps(report, indent=1, ensure_ascii=False) + "\n")
    return 0 if passed == total else 1


if __name__ == "__main__":
    sys.exit(main())
