"""Compare DACAD's results with the engine's for a batch of calculated files.

For every file that tools/dacad_batch.py calculated, runs the engine on the
original input and checks two things:

* every DACAD roof line has an engine line of the same type whose end points
  lie within --tol of it (either direction), and vice versa;
* the line totals per DACAD code agree within --tol (verges as OL+OR+OS).

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


def compare(calc: Path, src: Path, engine: str, tol: float) -> dict:
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
    dac = [(_type(LINE_TYPES[ln.type_code]), tuple(ln.a), tuple(ln.b))
           for ln in p.lines if ln.type_code in LINE_TYPES and math.dist(ln.a, ln.b) > 1e-3]
    if not dac:
        return {"ok": False, "error": "DACAD produced no roof lines"}
    te, td = Counter(), Counter()
    for t, a, b in eng:
        te[t] += math.dist(a, b)
    for t, a, b in dac:
        td[t] += math.dist(a, b)
    totals = {t: (round(td[t], 3), round(te[t], 3)) for t in sorted(set(te) | set(td))
              if abs(td[t] - te[t]) > tol}
    only_d, only_e = _match(dac, eng, tol), _match(eng, dac, tol)
    r = lambda ls: [(t, [round(c, 3) for c in a], [round(c, 3) for c in b]) for t, a, b in ls]
    return {"ok": not (totals or only_d or only_e), "totals": totals,
            "only_dacad": r(only_d), "only_engine": r(only_e)}


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("calc", type=Path)
    ap.add_argument("--inputs", required=True, type=Path)
    ap.add_argument("--engine", required=True)
    ap.add_argument("--tol", type=float, default=0.015)
    ap.add_argument("--json", type=Path)
    a = ap.parse_args(argv)
    status = json.loads((a.calc / "status.json").read_text())
    report, n_ok, n_fail, skipped = {}, 0, 0, Counter()
    for name, st in sorted(status.items()):
        if st["status"] != "ok":
            skipped[st["status"]] += 1
            continue
        res = compare(a.calc / name, a.inputs / name, a.engine, a.tol)
        report[name] = res
        if res["ok"]:
            n_ok += 1
        else:
            n_fail += 1
            if "error" in res:
                print(f"FAIL {name}: {res['error']}")
            else:
                print(f"FAIL {name}: totals {res['totals']} "
                      f"only DACAD {len(res['only_dacad'])}, only engine {len(res['only_engine'])}")
    print(f"compared {n_ok + n_fail}: {n_ok} identical, {n_fail} different; "
          f"not calculated by DACAD: {dict(skipped)}")
    if a.json:
        a.json.write_text(json.dumps(report, indent=1))
    return 1 if n_fail else 0


if __name__ == "__main__":
    raise SystemExit(main())
