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
    prio: int = 0  # horizontal priority (help: "Priorität für Horizontale")


@dataclass
class W:
    """Wall from the current point, *length* m in direction *deg*."""
    length: float
    deg: float
    base: Def
    stacked: list[Def] = field(default_factory=list)
    # Roof windows on this wall's plane: (x, y, width, height); x, y in plan.
    windows: list[tuple[float, float, float, float]] = field(default_factory=list)


@dataclass
class Chimney:
    height: float
    corners: list[tuple[float, float]]  # plan polygon


def rect(lx: float, ly: float, south: Def, east: Def, north: Def, west: Def,
         stacked: dict[str, list[Def]] | None = None) -> list[W]:
    s = stacked or {}
    return [W(lx, 0, south, s.get("S", [])), W(ly, 90, east, s.get("E", [])),
            W(lx, 180, north, s.get("N", [])), W(ly, 270, west, s.get("W", []))]


G = Def(90.0)  # gable wall


@dataclass
class Dormer:
    """A dormer is its own roof (help: "Gauben werden generell als eigenes
    Dach"): an open chain of walls from *start*, cut against the main roof.
    *kind* and *host* are the two values of its 0x11 roof record, copied
    from DACAD's samples (TRAING01: 1, 3)."""
    start: tuple[float, float]
    walls: list[W]
    kind: int = 1
    host: int = 3


def square(x: float, y: float, a: float) -> list[tuple[float, float]]:
    return [(x, y), (x + a, y), (x + a, y + a), (x, y + a)]


def rotated_rect(cx: float, cy: float, a: float, b: float,
                 deg: float) -> list[tuple[float, float]]:
    """Plan rectangle a × b centred on (cx, cy), side a turned *deg* from x."""
    c, s = math.cos(math.radians(deg)), math.sin(math.radians(deg))
    pts = [(-a / 2, -b / 2), (a / 2, -b / 2), (a / 2, b / 2), (-a / 2, b / 2)]
    return [(cx + x * c - y * s, cy + x * s + y * c) for x, y in pts]


def cases() -> dict[str, tuple]:
    p40, p35 = Def(40.0), Def(35.0)
    def notch(back: float, prio_back: int = 0, prio_sides: int = 0,
              bump: bool = False) -> list[W]:
        # 8 x 5 hip with a 2 x 1 notch in the south wall, or a 2 x 1 bump out
        # of it (the two geometries of DACAD's PRIORITY.DAD).
        side, b = Def(30, prio=prio_sides), Def(back, prio=prio_back)
        d1, d2 = (270, 90) if bump else (90, 270)
        return [W(3, 0, side), W(1, d1, G), W(2, 0, b), W(1, d2, G),
                W(3, 0, side), W(5, 90, Def(30)), W(8, 180, Def(30)), W(5, 270, Def(30))]

    def hip_with(windows: dict[int, list] | None = None) -> list[W]:
        ws = rect(12, 8, p35, p35, p35, p35)
        for i, wins in (windows or {}).items():
            ws[i].windows = wins
        return ws

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
        # Horizontal priority between a notch's back wall and its neighbours.
        "C18_notch50": ("Zářez 2×1 v jižní stěně, zadní stěna 50°, priority 0", notch(50)),
        "C19_notch50_back1": ("Totéž, zadní stěna zářezu priorita 1", notch(50, prio_back=1)),
        "C20_notch50_sides1": ("Totéž, jižní stěny vedle zářezu priorita 1", notch(50, prio_sides=1)),
        "C21_bump20": ("Výstupek 2×1 z jižní stěny, čelo 20°, priority 0", notch(20, bump=True)),
        "C22_bump20_front1": ("Totéž, čelo výstupku priorita 1", notch(20, prio_back=1, bump=True)),
        "C23_bump20_sides1": ("Totéž, jižní stěny vedle výstupku priorita 1",
                              notch(20, prio_sides=1, bump=True)),
        # Openings in the C04 hip (12 x 8, 35°, ridge y = 4 from x = 4 to 8).
        "C24_window": ("Valbová C04 + střešní okno 1,1×1,1 na jižní ploše",
                       hip_with(windows={0: [(6.0, 1.5, 1.1, 1.1)]})),
        "C25_window_tall": ("Valbová C04 + střešní okno 0,8×1,4 na jižní ploše",
                            hip_with(windows={0: [(6.0, 1.5, 0.8, 1.4)]})),
        "C26_window_small": ("Valbová C04 + malé okno 0,5×0,5 (pod limity norem)",
                             hip_with(windows={0: [(6.0, 1.5, 0.5, 0.5)]})),
        "C27_chimney": ("Valbová C04 + komín 0,6×0,6 v jižní ploše, výška 9 m",
                        hip_with(), [Chimney(9.0, square(5.7, 1.7, 0.6))]),
        "C28_chimney_ridge": ("Valbová C04 + komín 0,6×0,6 přes hřeben, výška 9 m",
                              hip_with(), [Chimney(9.0, square(5.7, 3.7, 0.6))]),
        # Gable dormer like TRAING01's: front wall on the eave wall line,
        # zero-length side walls whose direction sets the side planes.
        "C29_dormer_gable": ("Sedlová 12×8 40° + sedlový vikýř šířky 2 m, okap 4,0 m, 45°",
                             rect(12, 8, Def(40), G, Def(40), G), None,
                             [Dormer((5.0, 0.0), [W(0, 270, Def(45, 0, 4.0)),
                                                  W(2, 0, Def(90, 0, 4.0)),
                                                  W(0, 90, Def(45, 0, 4.0))])]),
        "C30_dormer_gable_setback": ("Totéž, čelo vikýře 1 m za okapovou stěnou",
                                     rect(12, 8, Def(40), G, Def(40), G), None,
                                     [Dormer((5.0, 1.0), [W(0, 270, Def(45, 0, 4.0)),
                                                          W(2, 0, Def(90, 0, 4.0)),
                                                          W(0, 90, Def(45, 0, 4.0))])]),
        "C31_dormer_shed": ("Sedlová 12×8 40° + pultový vikýř šířky 3 m, 15°, okap 5,0 m, čelo 1 m za stěnou",
                            rect(12, 8, Def(40), G, Def(40), G), None,
                            [Dormer((4.5, 1.0), [W(0, 270, Def(90, 0, 5.0)),
                                                 W(3, 0, Def(15, 0, 5.0)),
                                                 W(0, 90, Def(90, 0, 5.0))])]),
        # Oblique chimneys: which side counts as WV / WH / WS.
        "C32_chimney_rot30": ("Valbová C04 + komín 0,6×0,6 pootočený o 30°",
                              hip_with(), [Chimney(9.0, rotated_rect(6.0, 2.0, 0.6, 0.6, 30))]),
        "C33_chimney_rect_rot40": ("Valbová C04 + komín 1,0×0,4 pootočený o 40°",
                                   hip_with(), [Chimney(9.0, rotated_rect(6.0, 2.0, 1.0, 0.4, 40))]),
        "C34_chimney_rect_rot50": ("Valbová C04 + komín 1,0×0,4 pootočený o 50°",
                                   hip_with(), [Chimney(9.0, rotated_rect(6.0, 2.0, 1.0, 0.4, 50))]),
        "C35_chimney_rot2": ("Valbová C04 + komín 0,6×0,6 pootočený o 2°",
                             hip_with(), [Chimney(9.0, rotated_rect(6.0, 2.0, 0.6, 0.6, 2))]),
        "C36_chimney_rot10": ("Valbová C04 + komín 0,6×0,6 pootočený o 10°",
                              hip_with(), [Chimney(9.0, rotated_rect(6.0, 2.0, 0.6, 0.6, 10))]),
        # Openings around the norm limits (DIN 2.5 m², ÖNORM 4 m², SIA 1 m²).
        "C37_window_3m2": ("Valbová C04 + okno 2,0×1,6 (3,2 m²)",
                           hip_with(windows={0: [(5.0, 0.8, 2.0, 1.6)]})),
        "C38_window_5m2": ("Valbová C04 + okno 2,4×2,0 (4,8 m²)",
                           hip_with(windows={0: [(4.8, 0.5, 2.4, 2.0)]})),
        # More dormers (main: gable 12 x 8, 40°, unless noted).
        "C39_dormer_hip": ("Valbový vikýř 2 m, 45°, okap 4,0 m, čelo 1 m za stěnou",
                           rect(12, 8, Def(40), G, Def(40), G), None,
                           [Dormer((5.0, 1.0), [W(0, 270, Def(45, 0, 4.0)),
                                                W(2, 0, Def(45, 0, 4.0)),
                                                W(0, 90, Def(45, 0, 4.0))])]),
        "C40_dormer_overhang": ("Sedlový vikýř jako C30, přesah 0,3 na všech stěnách vikýře",
                                rect(12, 8, Def(40), G, Def(40), G), None,
                                [Dormer((5.0, 1.0), [W(0, 270, Def(45, 0.3, 4.0)),
                                                     W(2, 0, Def(90, 0.3, 4.0)),
                                                     W(0, 90, Def(45, 0.3, 4.0))])]),
        "C41_dormer_over_hip": ("Valbová C04 + sedlový vikýř 2 m přes nároží (x 2–4, čelo y 1, okap 3,8)",
                                hip_with(), None,
                                [Dormer((2.0, 1.0), [W(0, 270, Def(45, 0, 3.8)),
                                                     W(2, 0, Def(90, 0, 3.8)),
                                                     W(0, 90, Def(45, 0, 3.8))])]),
        "C42_dormer_over_ridge": ("Sedlová 12×8 40° + sedlový vikýř přes hřeben (čelo y 2, okap 6,5)",
                                  rect(12, 8, Def(40), G, Def(40), G), None,
                                  [Dormer((5.0, 2.0), [W(0, 270, Def(45, 0, 6.5)),
                                                       W(2, 0, Def(90, 0, 6.5)),
                                                       W(0, 90, Def(45, 0, 6.5))])]),
    }


def _rec(tag: int, payload: bytes) -> bytes:
    return struct.pack(">H", tag) + struct.pack("<H", len(payload)) + payload


def _material(d: Def) -> int:
    return 0 if abs(d.pitch - 90.0) < 1e-9 else 1


def _wall_rec(i: int, roof: int, x: float, y: float, w: W) -> list[bytes]:
    a, b = math.radians(w.deg), w.base
    out = [_rec(0x0012, struct.pack(
        "<HH4fHH3ffBB", i, roof, x, y, w.length, a, 0, _material(b),
        b.eave_h, b.overhang, b.pitch, float(b.prio), len(w.stacked),
        len(w.windows)))]
    for d in w.stacked:
        out.append(_rec(0x1012, struct.pack("<HH3f", 0, _material(d),
                                            d.eave_h, d.overhang, d.pitch)))
    for win in w.windows:
        out.append(_rec(0x2012, struct.pack("<4f", *win)))
    return out


def encode(walls: list[W], chimneys: list[Chimney] | None = None,
           dormers: list[Dormer] | None = None) -> bytes:
    chimneys, dormers = chimneys or [], dormers or []
    n_walls = len(walls) + sum(len(d.walls) for d in dormers)
    out = [b"DACAD\x1a",
           _rec(0x0001, b"DACAD V1.13\0"),
           _rec(0x0002, b"$Header: tectora dacad_testcases.py $\0"),
           # Scale table copied from DACAD's own V1.13 samples.
           _rec(0x0003, struct.pack("<15H", 1, 1000, 2000, 3000, 12000, 5000,
                                    6000, 7000, 8000, 9000, 10000, 0, 0, 0, 0)),
           _rec(0x0004, bytes.fromhex("4111d42d")),
           _rec(0x0005, bytes.fromhex("7b247e2e")),
           _rec(0x0006, b"1.29H\0"),
           _rec(0x0010, struct.pack("<3H", 1 + len(dormers), n_walls, len(chimneys))),
           _rec(0x0011, struct.pack("<3H", 1, 0, 0))]
    for r, d in enumerate(dormers, start=2):
        out.append(_rec(0x0011, struct.pack("<3H", r, d.kind, d.host)))
    x = y = 0.0
    for i, w in enumerate(walls, start=1):
        out += _wall_rec(i, 1, x, y, w)
        x += w.length * math.cos(math.radians(w.deg))
        y += w.length * math.sin(math.radians(w.deg))
    if math.hypot(x, y) > 1e-6:
        raise ValueError(f"footprint not closed, ends at ({x:.3f}, {y:.3f})")
    i = len(walls)
    for r, d in enumerate(dormers, start=2):
        x, y = d.start
        for w in d.walls:
            i += 1
            out += _wall_rec(i, r, x, y, w)
            x += w.length * math.cos(math.radians(w.deg))
            y += w.length * math.sin(math.radians(w.deg))
    for i, c in enumerate(chimneys, start=1):
        out.append(_rec(0x0013, struct.pack("<HfH", i, c.height, len(c.corners))))
        for cx, cy in c.corners:
            out.append(_rec(0x1013, struct.pack("<2f", cx, cy)))
    out.append(_rec(0xFFFF, b""))
    return b"".join(out)


def main(argv: list[str]) -> int:
    out = Path(argv[0] if argv else "dacad-testcases")
    out.mkdir(parents=True, exist_ok=True)
    index = []
    for name, (desc, walls, *rest) in cases().items():
        (out / f"{name}.DAD").write_bytes(encode(walls, *rest))
        index.append(f"{name}.DAD  {desc}")
    (out / "README.txt").write_text("\n".join(index) + "\n", encoding="utf-8")
    print("\n".join(index))
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
