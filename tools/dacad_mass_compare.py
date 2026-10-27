"""Compare DACAD's results with the engine's for a batch of calculated files.

For every file that tools/dacad_batch.py calculated, runs the engine on the
original input and checks:

* every positive DACAD roof line has an engine line of the same type whose
  end points lie within --tol of it (either direction), and vice versa;
* every negative DACAD line (what a hole cuts out of a line) has an engine
  minus line of the same type and length within --tol, and vice versa;
* the line totals per DACAD code agree within --tol (verges as OL+OR+OS),
  summing DACAD's lengths with their sign, as its own totals do;
* the roof area agrees within --area-tol (DACAD: its stored area groups
  other than the walls, "Stena"/"Wand", i.e. net of holes and cut-outs).

    python tools/dacad_mass_compare.py CALC_DIR --inputs IN_DIR \\
        --engine "tectora calc {file}" [--json report.json]

Prints one line per failing file and a summary; exit code 1 if any fails.
"""
from __future__ import annotations

import argparse
import json
import math
import shlex
import subprocess
import sys
from collections import Counter
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))
from tectora.io.dacad_dad import LINE_TYPES, read_dad  # noqa: E402

KIND = {"eave": "T", "ridge": "F", "hip": "G", "valley": "K", "knick": "c",
        "pult": "P", "abutment_front": "WV", "abutment_side": "WS",
        "abutment_back": "WH", "verge": "O"}
VERGES = {"OL", "OR", "OS"}


def _type(code: str) -> str:
    return "O" if code in VERGES else code


def _near(p, q, tol) -> bool:
    return all(abs(a - b) <= tol for a, b in zip(p, q))


def _match(xs, ys, tol):
    """Lines of xs with no counterpart in ys (same type, ends within tol)."""
    left = list(ys)
    miss = []
    for t, a, b in xs:
        for j, (u, c, d) in enumerate(left):
            if u == t and ((_near(a, c, tol) and _near(b, d, tol)) or
                           (_near(a, d, tol) and _near(b, c, tol))):
                left.pop(j)
                break
        else:
            miss.append((t, a, b))
    return miss


def _match_len(xs, ys, tol):
    """Deductions (type, length) of xs with no counterpart in ys."""
    left = list(ys)
    miss = []
    for t, ln in xs:
        j = next((j for j, (u, m) in enumerate(left) if u == t and abs(m - ln) <= tol), None)
        if j is None:
            miss.append((t, round(ln, 3)))
        else:
            left.pop(j)
    return miss


def _raised_eave_snap(raw, tol=0.025) -> bool:
    """DACAD quirk: where a raised eave meets a lower neighbour's hip within
    ~2 cm of the neighbour's wall corner, DACAD snaps the hip end to that
    corner (sweep /mnt/project-files/dacad-raised-eave/), leaving a hip end
    a few mm to 2.5 cm from the eave end, and its face areas there are not
    even symmetric. Such failures are labelled, still counted as failures."""
    hips = [e for t, a, b, _ in raw if t == "G" for e in (a, b)]
    eaves = [e for t, a, b, _ in raw if t == "T" for e in (a, b)]
    return any(1e-3 < math.dist(g[:2], e[:2]) < tol and abs(g[2] - e[2]) < 0.02
               for g in hips for e in eaves)


def compare(calc: Path, src: Path, engine: str, tol: float, area_tol: float) -> dict:
    argv = [x.replace("{file}", str(src)) for x in shlex.split(engine)]
    r = subprocess.run(argv, capture_output=True, text=True)
    try:
        data = json.loads(r.stdout)
    except json.JSONDecodeError:
        return {"ok": False, "error": "engine output: " + (r.stderr or r.stdout)[:200]}
    if not data.get("ok", True) or "parts" not in data:
        return {"ok": False, "error": "engine: " + str(data.get("error"))[:200]}
    eng = [(KIND.get(ln["kind"], ln["kind"]), tuple(ln["a"]), tuple(ln["b"]))
           for p in data["parts"] for ln in p["geometry"]["lines"]
           if math.dist(ln["a"], ln["b"]) > 1e-3]
    p = read_dad(str(calc))
    raw = [(_type(LINE_TYPES[ln.type_code]), tuple(ln.a), tuple(ln.b), ln.length_m)
           for ln in p.lines if ln.type_code in LINE_TYPES and math.dist(ln.a, ln.b) > 1e-3]
    # DACAD keeps a line whole and stores what a hole cuts out of it as a
    # negative line of the same type; the engine does the same (whole lines
    # plus minus_lines as (code, length)). Compare positives by geometry and
    # deductions by type and length.
    dac = [(t, a, b) for t, a, b, ln in raw if ln >= 0]
    if not dac:
        return {"ok": False, "error": "DACAD produced no roof lines"}
    dmin = [(t, -ln) for t, a, b, ln in raw if ln < 0]
    emin = [(_type(code), ln) for pt in data["parts"]
            for code, ln in pt["geometry"].get("minus_lines", [])]
    te, td = Counter(), Counter()
    for t, a, b in eng:
        te[t] += math.dist(a, b)
    for t, ln in emin:
        te[t] -= ln
    for t, a, b, ln in raw:
        td[t] += math.copysign(math.dist(a, b), ln)
    totals = {t: (round(td[t], 3), round(te[t], 3)) for t in sorted(set(te) | set(td))
              if abs(td[t] - te[t]) > tol}
    only_d, only_e = _match(dac, eng, tol), _match(eng, dac, tol)
    r = lambda ls: [(t, [round(c, 3) for c in a], [round(c, 3) for c in b]) for t, a, b in ls]
    res = {"totals": totals, "only_dacad": r(only_d), "only_engine": r(only_e),
           "minus_only_dacad": _match_len(dmin, emin, tol),
           "minus_only_engine": _match_len(emin, dmin, tol)}
    ea = sum(pt["quantities"].get("roof_area", 0.0) for pt in data["parts"])
    roof = [v for k, v in p.area_groups.items() if not k.startswith(("Stena", "Wand"))]
    da = sum(roof) if roof else None
    if da is not None and abs(da - ea) > area_tol:
        res["area"] = (round(da, 4), round(ea, 4))
    res["ok"] = not (totals or only_d or only_e or res["minus_only_dacad"]
                     or res["minus_only_engine"] or "area" in res)
    if not res["ok"] and _raised_eave_snap(raw):
        res["quirk"] = "raised-eave snap"
    return res


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("calc", type=Path)
    ap.add_argument("--inputs", required=True, type=Path)
    ap.add_argument("--engine", required=True)
    ap.add_argument("--tol", type=float, default=0.015)
    ap.add_argument("--area-tol", type=float, default=0.01)
    ap.add_argument("--json", type=Path)
    a = ap.parse_args(argv)
    status = json.loads((a.calc / "status.json").read_text())
    report, n_ok, n_fail, n_quirk, skipped = {}, 0, 0, 0, Counter()
    for name, st in sorted(status.items()):
        if st["status"] != "ok":
            skipped[st["status"]] += 1
            continue
        res = compare(a.calc / name, a.inputs / name, a.engine, a.tol, a.area_tol)
        report[name] = res
        if res["ok"]:
            n_ok += 1
        else:
            n_quirk += "quirk" in res
            n_fail += 1
            if "error" in res:
                print(f"FAIL {name}: {res['error']}")
            else:
                print(f"FAIL {name}: [{res.get('quirk', '')}] totals {res['totals']} area {res.get('area')} "
                      f"only DACAD {len(res['only_dacad'])}, only engine {len(res['only_engine'])}")
    print(f"compared {n_ok + n_fail}: {n_ok} identical, {n_fail} different "
          f"({n_quirk} of them the raised-eave snap); "
          f"not calculated by DACAD: {dict(skipped)}")
    if a.json:
        a.json.write_text(json.dumps(report, indent=1))
    return 1 if n_fail else 0


if __name__ == "__main__":
    raise SystemExit(main())
