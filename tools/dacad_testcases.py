"""Write small DACAD V1.13 projects that isolate one roof rule each.

DACAD's sample projects are all complex (mixed pitches, dormers, windows), so a
mismatch there is hard to pin on one rule. These cases change one thing at a
time. Open each file in DACAD, run the calculation (Ampel / Strg+B) and save:
DACAD then stores its results in the file, and ``tools/dacad_golden.py
export`` turns them into golden fixtures.

    python tools/dacad_testcases.py out/dacad-testcases

All cases: roof no. 1, eave height 3.0 m, material tiles unless the wall is a
gable (pitch 90°, no material). Walls run counter-clockwise; DACAD puts the
roof plane on the left of each wall.
"""
from __future__ import annotations

import math
import struct
import sys
from dataclasses import dataclass, field
from pathlib import Path

EAVE_H = 3.0


@dataclass
class Def:
    pitch: float
    overhang: float = 0.0
    eave_h: float = EAVE_H


@dataclass
class W:
    """Wall from the current point, *length* m in direction *deg*."""
    length: float
    deg: float
    base: Def
    stacked: list[Def] = field(default_factory=list)


def rect(lx: float, ly: float, south: Def, east: Def, north: Def, west: Def,
         stacked: dict[str, list[Def]] | None = None) -> list[W]:
    s = stacked or {}
    return [W(lx, 0, south, s.get("S", [])), W(ly, 90, east, s.get("E", [])),
            W(lx, 180, north, s.get("N", [])), W(ly, 270, west, s.get("W", []))]


G = Def(90.0)  # gable wall


def cases() -> dict[str, tuple[str, list[W]]]:
    p40, p35 = Def(40.0), Def(35.0)
    return {
        "C01_gable": ("Sedlová 10×8, 40°, bez přesahu", rect(10, 8, p40, G, p40, G)),
        "C02_gable_overhang_all": ("Sedlová 10×8, 40°, přesah 0,5 na všech stěnách",
                                   rect(10, 8, Def(40, 0.5), Def(90, 0.5), Def(40, 0.5), Def(90, 0.5))),
        "C03_gable_overhang_eaves": ("Sedlová 10×8, 40°, přesah 0,5 jen u okapů",
                                     rect(10, 8, Def(40, 0.5), G, Def(40, 0.5), G)),
        "C04_hip": ("Valbová 12×8, 35°", rect(12, 8, p35, p35, p35, p35)),
        "C05_hip_overhang": ("Valbová 12×8, 35°, přesah 0,6",
                             rect(12, 8, *(Def(35, 0.6),) * 4)),
        "C06_pyramid": ("Stanová 8×8, 45°", rect(8, 8, *(Def(45),) * 4)),
        "C07_shed": ("Pultová 10×6, 25° (okap na jihu)", rect(10, 6, Def(25), G, G, G)),
        "C08_half_hip": ("Polovalba: sedlová 12×8 45°, valbičky od 5,5 m sklon 60°",
                         rect(12, 8, Def(45), G, Def(45), G,
                              {"E": [Def(60, 0, 5.5)], "W": [Def(60, 0, 5.5)]})),
        "C09_hip_mixed_pitch": ("Valbová 12×8, boky 30°, valby 50°",
                                rect(12, 8, Def(30), Def(50), Def(30), Def(50))),
        "C10_hip_eave_heights": ("Valbová 12×8, 35°, severní okap ve výšce 3,5 m",
                                 rect(12, 8, p35, p35, Def(35, 0, 3.5), p35)),
        "C11_L_hip": ("L 10×10, křídla 4 m, valbová 35°",
                      [W(10, 0, p35), W(4, 90, p35), W(6, 180, p35), W(6, 90, p35),
                       W(4, 180, p35), W(10, 270, p35)]),
        "C12_L_gable_ends": ("L 10×10, křídla 4 m, 35°, štíty na koncích křídel",
                             [W(10, 0, p35), W(4, 90, G), W(6, 180, p35), W(6, 90, p35),
                              W(4, 180, G), W(10, 270, p35)]),
        "C13_gable_asym": ("Sedlová 10×8, jih 30°, sever 45°", rect(10, 8, Def(30), G, Def(45), G)),
        "C14_mansard": ("Mansardová valbová 12×8: 60° do 5,0 m, nad tím 30°",
                        rect(12, 8, *(Def(60),) * 4,
                             {k: [Def(30, 0, 5.0)] for k in "SENW"})),
        # Asymmetric cases to tell OL (verge at the plane's start) from OR.
        "C15_shed_steep": ("Pultová 10×6, 40°, okap jen na jihu", rect(10, 6, Def(40), G, G, G)),
        "C16_hip_east_high": ("Valbová 12×8, 35°, východní okap ve výšce 3,5 m",
                              rect(12, 8, p35, Def(35, 0, 3.5), p35, p35)),
        "C17_shed_trapezoid": ("Pultová na lichoběžníku (0,0)-(10,0)-(8,6)-(0,6), 25°, okap na jihu",
                               [W(10, 0, Def(25)),
                                W(math.hypot(2, 6), math.degrees(math.atan2(6, -2)), G),
                                W(8, 180, G), W(6, 270, G)]),
    }


def _rec(tag: int, payload: bytes) -> bytes:
    return struct.pack(">H", tag) + struct.pack("<H", len(payload)) + payload


def _material(d: Def) -> int:
    return 0 if abs(d.pitch - 90.0) < 1e-9 else 1


def encode(walls: list[W]) -> bytes:
    out = [b"DACAD\x1a",
           _rec(0x0001, b"DACAD V1.13\0"),
           _rec(0x0002, b"$Header: tectora dacad_testcases.py $\0"),
           # Scale table copied from DACAD's own V1.13 samples.
           _rec(0x0003, struct.pack("<15H", 1, 1000, 2000, 3000, 12000, 5000,
                                    6000, 7000, 8000, 9000, 10000, 0, 0, 0, 0)),
           _rec(0x0004, bytes.fromhex("4111d42d")),
           _rec(0x0005, bytes.fromhex("7b247e2e")),
           _rec(0x0006, b"1.29H\0"),
           _rec(0x0010, struct.pack("<3H", 1, len(walls), 0)),
           _rec(0x0011, struct.pack("<3H", 1, 0, 0))]
    x = y = 0.0
    for i, w in enumerate(walls, start=1):
        a = math.radians(w.deg)
        b = w.base
        out.append(_rec(0x0012, struct.pack(
            "<HH4fHH3ffBB", i, 1, x, y, w.length, a, 0, _material(b),
            b.eave_h, b.overhang, b.pitch, 0.0, len(w.stacked), 0)))
        for d in w.stacked:
            out.append(_rec(0x1012, struct.pack("<HH3f", 0, _material(d),
                                                d.eave_h, d.overhang, d.pitch)))
        x += w.length * math.cos(a)
        y += w.length * math.sin(a)
    if math.hypot(x, y) > 1e-6:
        raise ValueError(f"footprint not closed, ends at ({x:.3f}, {y:.3f})")
    out.append(_rec(0xFFFF, b""))
    return b"".join(out)


def main(argv: list[str]) -> int:
    out = Path(argv[0] if argv else "dacad-testcases")
    out.mkdir(parents=True, exist_ok=True)
    index = []
    for name, (desc, walls) in cases().items():
        (out / f"{name}.DAD").write_bytes(encode(walls))
        index.append(f"{name}.DAD  {desc}")
    (out / "README.txt").write_text("\n".join(index) + "\n", encoding="utf-8")
    print("\n".join(index))
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
