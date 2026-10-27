"""Write a V1 .DAD file from a decoded project, optionally with parts removed.

Used to bisect a mismatch on a complex sample: drop chimneys, zero-length
walls or whole roofs, let DACAD calculate each variant
(``dacad_wine_calc.sh``) and compare again.

    python tools/dacad_variants.py SAMPLE.DAD out/ [--no-chimneys]
        [--no-markers] [--roofs 1]

Windows are not written (the V1 reader does not keep their wall).
"""
from __future__ import annotations

import argparse
import math
import struct
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))
sys.path.insert(0, str(Path(__file__).resolve().parent))

from dacad_testcases import _rec  # noqa: E402
from tectora.io.dacad_dad import DadProject, read_dad  # noqa: E402


def encode_project(p: DadProject, walls=None, chimneys=None) -> bytes:
    walls = p.walls if walls is None else walls
    chimneys = p.chimneys if chimneys is None else chimneys
    roofs = sorted({w.definitions[0].roof_no for w in walls})
    out = [b"DACAD\x1a",
           _rec(0x0001, b"DACAD V1.13\0"),
           _rec(0x0002, b"$Header: tectora dacad_variants.py $\0"),
           _rec(0x0003, struct.pack("<15H", 1, 1000, 2000, 3000, 12000, 5000,
                                    6000, 7000, 8000, 9000, 10000, 0, 0, 0, 0)),
           _rec(0x0004, bytes.fromhex("4111d42d")),
           _rec(0x0005, bytes.fromhex("7b247e2e")),
           _rec(0x0006, b"1.29H\0"),
           _rec(0x0010, struct.pack("<3H", len(roofs), len(walls), len(chimneys)))]
    for r in roofs:
        # Dormer roofs as in TRAING01 (kind 1, host 3).
        out.append(_rec(0x0011, struct.pack("<3H", r, *((0, 0) if r == 1 else (1, 3)))))
    for i, w in enumerate(walls, start=1):
        d = w.definitions[0]
        direction = w.direction_deg if w.direction_deg is not None else math.degrees(
            math.atan2(w.end[1] - w.start[1], w.end[0] - w.start[0]))
        out.append(_rec(0x0012, struct.pack(
            "<HH4fHH3ffBB", i, d.roof_no, w.start[0], w.start[1], w.length,
            math.radians(direction), 0, d.material, d.eave_height_m, d.overhang_m,
            d.pitch_deg, float(d.priority), len(w.definitions) - 1, 0)))
        for s in w.definitions[1:]:
            out.append(_rec(0x1012, struct.pack("<HH3f", 0, s.material, s.eave_height_m,
                                                s.overhang_m, s.pitch_deg)))
    for i, c in enumerate(chimneys, start=1):
        out.append(_rec(0x0013, struct.pack("<HfH", i, c.height_m, len(c.corners))))
        for x, y in c.corners:
            out.append(_rec(0x1013, struct.pack("<2f", x, y)))
    out.append(_rec(0xFFFF, b""))
    return b"".join(out)


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("src", type=Path)
    ap.add_argument("out", type=Path)
    ap.add_argument("--no-chimneys", action="store_true")
    ap.add_argument("--no-markers", action="store_true", help="drop zero-length walls")
    ap.add_argument("--roofs", type=int, nargs="*", help="keep only these roof numbers")
    a = ap.parse_args(argv)
    p = read_dad(str(a.src))
    walls = [w for w in p.walls
             if not (a.no_markers and w.length < 1e-6)
             and (a.roofs is None or w.definitions[0].roof_no in a.roofs)]
    tags = [t for t, on in (("nochim", a.no_chimneys), ("nomark", a.no_markers),
                            ("roofs" + "-".join(map(str, a.roofs or [])), a.roofs)) if on]
    a.out.mkdir(parents=True, exist_ok=True)
    dst = a.out / f"{a.src.stem}{'_' + '_'.join(tags) if tags else ''}.DAD"
    dst.write_bytes(encode_project(p, walls, [] if a.no_chimneys else None))
    print(dst)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
