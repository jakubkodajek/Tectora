"""Line-by-line diff between DACAD's result and an engine, for one project.

    python tools/dacad_linediff.py calculated.DAD --engine "tectora calc {file}" \\
        [--input original.DAD]

Prints every line that only one side has, or that the two classify
differently, with its length and rounded end points. The engine must print
JSON with ``parts[].geometry.lines[]`` entries {kind, a, b}.
"""
from __future__ import annotations

import argparse
import json
import math
import shlex
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from tectora.io.dacad_dad import LINE_TYPES, read_dad  # noqa: E402

# Engine kinds that map one-to-one to a DACAD code.
KIND = {"eave": "T", "ridge": "F", "hip": "G", "valley": "K"}


def _key(a, b) -> tuple:
    return tuple(sorted([tuple(round(c, 2) for c in a), tuple(round(c, 2) for c in b)]))


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("calculated", type=Path)
    ap.add_argument("--engine", required=True)
    ap.add_argument("--input", type=Path, help="file to give the engine (default: calculated)")
    a = ap.parse_args(argv)
    src = a.input or a.calculated
    argv_e = [x.replace("{file}", str(src)) for x in shlex.split(a.engine)]
    data = json.loads(subprocess.run(argv_e, capture_output=True, text=True, check=True).stdout)
    eng = {_key(ln["a"], ln["b"]): ln["kind"]
           for p in data["parts"] for ln in p["geometry"]["lines"]
           if math.dist(ln["a"], ln["b"]) > 1e-3}
    dac = {_key(ln.a, ln.b): LINE_TYPES[ln.type_code]
           for ln in read_dad(str(a.calculated)).lines
           if ln.type_code in LINE_TYPES and math.dist(ln.a, ln.b) > 1e-3}
    print(f"{'DACAD':5} {'engine':16} {'length':>7}  end points")
    for k in sorted(set(dac) | set(eng)):
        d, e = dac.get(k), eng.get(k)
        if d and e and KIND.get(e, e) == d:
            continue
        print(f"{d or '-':5} {e or '-':16} {math.dist(*k):7.3f}  {k}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
