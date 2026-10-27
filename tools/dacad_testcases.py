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
    # Roof number of an existing roof to add this chain to as another wall
    # loop (no 0x11 record of its own). DACAD rejects that with "Kein
    # geschlossener Grundriss"; a separate building is its own roof with
    # kind = host = 0 (TRAING07).
    roof: int | None = None


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
        # Rules found in TRAING07 (see docs/verification/dacad-parity.md).
        "C43_zero_wall_step": ("Sedlová 10×8 40°, přesah 0,5; západní štít: 4 m s přesahem 0,6, nulová stěna, 4 m bez přesahu",
                               [W(10, 0, Def(40, 0.5)), W(8, 90, Def(90, 0.5)), W(10, 180, Def(40, 0.5)),
                                W(4, 270, Def(90, 0.6)), W(0, 0, Def(90)), W(4, 270, Def(90))]),
        "C44_half_hip_partial_wall": ("Polovalba C08 (12×8 45°, valbička od 5,5 m 60°) jen na severních 5 m západního štítu",
                                      [W(12, 0, Def(45)), W(8, 90, G, [Def(60, 0, 5.5)]), W(12, 180, Def(45)),
                                       W(5, 270, G, [Def(60, 0, 5.5)]), W(3, 270, G)]),
        "C45_bump_higher_eave": ("Sedlová 12×8 40° přesah 0,5 + výstupek 4×2 z jižní stěny, okap 3,6, štít vpředu",
                                 [W(4, 0, Def(40, 0.5)), W(2, 270, Def(40, 0.5, 3.6)), W(4, 0, Def(90, 0.5, 3.6)),
                                  W(2, 90, Def(40, 0.5, 3.6)), W(4, 0, Def(40, 0.5)), W(8, 90, Def(90, 0.5)),
                                  W(12, 180, Def(40, 0.5)), W(8, 270, Def(90, 0.5))]),
        # Several wall loops (help: separate buildings are separate roofs).
        "C46_cross_two_roofs": ("Dva překrývající se obdélníky 12×6 a 6×12 (kříž), valbová 35°, střechy č. 1 a 2",
                                rect(12, 6, p35, p35, p35, p35), None,
                                [Dormer((3.0, -3.0), rect(6, 12, p35, p35, p35, p35), kind=0, host=0)]),
        "C47_cross_higher_eave": ("Jako C46, druhý obdélník s okapem 4,0 m",
                                  rect(12, 6, p35, p35, p35, p35), None,
                                  [Dormer((3.0, -3.0), rect(6, 12, *(Def(35, 0, 4.0),) * 4), kind=0, host=0)]),
        "C48_touching_wall": ("Valbové 8×6 a 6×6 vedle sebe, společná stěna x = 8, 35°",
                              rect(8, 6, p35, p35, p35, p35), None,
                              [Dormer((8.0, 0.0), rect(6, 6, p35, p35, p35, p35), kind=0, host=0)]),
        "C49_overhang_under_higher": ("Valbová 8×6 okap 3,0 přesah 0,5 vedle valbové 6×8 (od y −1) okap 4,5 přesah 0,5, 35°",
                                      rect(8, 6, *(Def(35, 0.5),) * 4), None,
                                      [Dormer((8.0, -1.0), rect(6, 8, *(Def(35, 0.5, 4.5),) * 4), kind=0, host=0)]),
        "C50_hip_unequal_overhang": ("Valbová 10×8 30°, přesah 0,5 na dlouhých a 0,2 na krátkých stěnách",
                                     rect(10, 8, Def(30, 0.5), Def(30, 0.2), Def(30, 0.5), Def(30, 0.2))),
        "C51_hip_higher_short_eaves": ("Valbová 10×8 30°, přesah 0,5, krátké stěny okap 3,3",
                                       rect(10, 8, Def(30, 0.5), Def(30, 0.5, 3.3), Def(30, 0.5), Def(30, 0.5, 3.3))),
        # HINT04: a shed wing against a gable. Main: gable 10 x 8, 40°, gable overhang 0.4.
        "C52_wing_into_gable": ("Sedlová 10×8 40° (přesah štítů 0,4) + pultové křídlo 3×2 zasahující 1 m do domu přes východní štít",
                                rect(10, 8, Def(40), Def(90, 0.4), Def(40), Def(90, 0.4)), None,
                                [Dormer((9.0, 3.0), [W(3, 0, G), W(2, 90, Def(20, 0.2, 3.5)), W(3, 180, G),
                                                     W(2, 270, Def(90, 0.1))], kind=0, host=0)]),
        "C53_wing_touching_gable": ("Jako C52, křídlo začíná přesně na štítové stěně (x 10–13)",
                                    rect(10, 8, Def(40), Def(90, 0.4), Def(40), Def(90, 0.4)), None,
                                    [Dormer((10.0, 3.0), [W(3, 0, G), W(2, 90, Def(20, 0.2, 3.5)), W(3, 180, G),
                                                          W(2, 270, Def(90, 0.1))], kind=0, host=0)]),
        "C54_wing_under_overhang": ("Jako C52, křídlo 0,2 m od štítu, pod jeho přesahem (x 10,2–13)",
                                    rect(10, 8, Def(40), Def(90, 0.4), Def(40), Def(90, 0.4)), None,
                                    [Dormer((10.2, 3.0), [W(2.8, 0, G), W(2, 90, Def(20, 0.2, 3.5)), W(2.8, 180, G),
                                                          W(2, 270, Def(90, 0.1))], kind=0, host=0)]),
        # ZWEIPULT roof 3: a lean-to box inside a shed roof's gable overhang.
        "C55_box_in_gable_overhang": ("Pultová 10×8 20° (okap jih, přesah 0,5; štíty 0,5; horní štít sever přesah 1,0) + box 2×1 v přesahu (x 6–8, y 8–9), západní stěna 45° okap 4,0",
                                      rect(10, 8, Def(20, 0.5), Def(90, 0.5), Def(90, 1.0), Def(90, 0.5)), None,
                                      [Dormer((6.0, 9.0), [W(1, 270, Def(45, 0, 4.0)), W(2, 0, G), W(1, 90, G),
                                                           W(2, 180, G)], kind=0, host=0)]),
        "C56_box_beyond_overhang": ("Jako C55, ale severní přesah 0,5 (box z poloviny mimo přesah)",
                                    rect(10, 8, Def(20, 0.5), Def(90, 0.5), Def(90, 0.5), Def(90, 0.5)), None,
                                    [Dormer((6.0, 9.0), [W(1, 270, Def(45, 0, 4.0)), W(2, 0, G), W(1, 90, G),
                                                         W(2, 180, G)], kind=0, host=0)]),
        # STEFFNER: higher wall (eave 4.33, overhang 0.6) then a lower one
        # (eave 3.35) at a reflex corner.
        "C57_reflex_higher_wall_ov0": ("L 12×10, výřez 6×4 vpravo dole; stěna nad výřezem okap 4,33 přesah 0,6, navazující nižší okap 3,35 přesah 0; 35°",
                                       [W(6, 0, Def(35, 0.6, 4.33)), W(4, 270, Def(35, 0, 3.35)), W(6, 0, Def(35, 0, 3.35)),
                                        W(10, 90, Def(35, 0, 3.35)), W(12, 180, Def(35, 0, 3.35)), W(6, 270, Def(35, 0.6, 4.33))]),
        "C58_reflex_higher_wall_ov06": ("Jako C57, nižší stěny s přesahem 0,6",
                                        [W(6, 0, Def(35, 0.6, 4.33)), W(4, 270, Def(35, 0.6, 3.35)), W(6, 0, Def(35, 0.6, 3.35)),
                                         W(10, 90, Def(35, 0.6, 3.35)), W(12, 180, Def(35, 0.6, 3.35)), W(6, 270, Def(35, 0.6, 4.33))]),
        # TRAING09: gable between 25° markers next to a raised wall.
        "C59_gable_markers_raised": ("Okap y 0 (okap 4,79, 50°, přesah 0,3): 4 m, značka 25°, štít 2 m, značka 25°, 1 m; pak nižší křídlo (okap 3,0) přes vnitřní roh",
                                     [W(4, 0, Def(50, 0.3, 4.79)), W(0, 270, Def(25, 0.3, 4.79)), W(2, 0, Def(90, 0.3, 4.79)),
                                      W(0, 90, Def(25, 0.3, 4.79)), W(1, 0, Def(50, 0.3, 4.79)), W(1.5, 270, Def(55, 0.3, 3.0)),
                                      W(5, 0, Def(90, 0.3, 3.0)), W(9.5, 90, Def(55, 0.3, 3.0)), W(12, 180, Def(50, 0.3, 3.0)),
                                      W(8, 270, Def(55, 0.3, 3.0))]),
        # ZWEIPULT boxes 2/3: a separate box whose sloped wall repeats a plane of the main roof.
        "C60_box_same_plane_inside": ("Sedlová 10×8 40° (okapy přesah 0,5, štíty 0,5) + box 2×1 uvnitř u jižní stěny, jeho jižní stěna = stejná rovina (40°, okap 3,0, přesah 0,5)",
                                      rect(10, 8, Def(40, 0.5), Def(90, 0.5), Def(40, 0.5), Def(90, 0.5)), None,
                                      [Dormer((4.0, 0.0), [W(2, 0, Def(40, 0.5)), W(1, 90, G), W(2, 180, G),
                                                           W(1, 270, G)], kind=0, host=0)]),
        "C61_box_same_plane_outside": ("Jako C60, box 2×1 vně za východním štítem (x 10–12), jižní stěna v linii a rovině hlavního okapu",
                                       rect(10, 8, Def(40, 0.5), Def(90, 0.5), Def(40, 0.5), Def(90, 0.5)), None,
                                       [Dormer((10.0, 0.0), [W(2, 0, Def(40, 0.5)), W(1, 90, G), W(2, 180, G),
                                                             W(1, 270, G)], kind=0, host=0)]),
        "C62_box_same_plane_past_ridge": ("Jako C60, box sahá od jižní stěny 1 cm za hřeben (y 0–4,01)",
                                          rect(10, 8, Def(40, 0.5), Def(90, 0.5), Def(40, 0.5), Def(90, 0.5)), None,
                                          [Dormer((4.0, 0.0), [W(2, 0, Def(40, 0.5)), W(4.01, 90, G), W(2, 180, G),
                                                               W(4.01, 270, G)], kind=0, host=0)]),
        "C63_box_same_plane_to_ridge": ("Jako C62, box končí přesně na hřebeni (y 0–4)",
                                        rect(10, 8, Def(40, 0.5), Def(90, 0.5), Def(40, 0.5), Def(90, 0.5)), None,
                                        [Dormer((4.0, 0.0), [W(2, 0, Def(40, 0.5)), W(4, 90, G), W(2, 180, G),
                                                             W(4, 270, G)], kind=0, host=0)]),
        "C64_box_same_plane_across_ridge": ("Jako C62, box přesahuje hřeben o 0,5 m (y 0–4,5)",
                                            rect(10, 8, Def(40, 0.5), Def(90, 0.5), Def(40, 0.5), Def(90, 0.5)), None,
                                            [Dormer((4.0, 0.0), [W(2, 0, Def(40, 0.5)), W(4.5, 90, G), W(2, 180, G),
                                                                 W(4.5, 270, G)], kind=0, host=0)]),
        "C65_box_same_plane_ov1_across_ridge": ("Jako C64, šikmá stěna boxu i hlavní okap s přesahem 1,0",
                                                rect(10, 8, Def(40, 1.0), Def(90, 0.5), Def(40, 1.0), Def(90, 0.5)), None,
                                                [Dormer((4.0, 0.0), [W(2, 0, Def(40, 1.0)), W(4.5, 90, G), W(2, 180, G),
                                                                     W(4.5, 270, G)], kind=0, host=0)]),
        "C66_box_same_plane_to_gable_edge": ("Jako C60 (hloubka 1 m), box x 9–10,5 až k přesahu východního štítu",
                                             rect(10, 8, Def(40, 0.5), Def(90, 0.5), Def(40, 0.5), Def(90, 0.5)), None,
                                             [Dormer((9.0, 0.0), [W(1.5, 0, Def(40, 0.5)), W(1, 90, G), W(1.5, 180, G),
                                                                  W(1, 270, G)], kind=0, host=0)]),
        "C67_box_same_plane_short_of_ridge": ("Jako C63, hloubka boxu 3,99 m (1 cm pod hřebenem)",
                                              rect(10, 8, Def(40, 0.5), Def(90, 0.5), Def(40, 0.5), Def(90, 0.5)), None,
                                              [Dormer((4.0, 0.0), [W(2, 0, Def(40, 0.5)), W(3.99, 90, G), W(2, 180, G),
                                                                   W(3.99, 270, G)], kind=0, host=0)]),
        # KRUPA006: gable of the main roof (overhang 1.0) beside a lower wing at a reflex corner.
        "C68_raised_gable_high_eave": ("L jako KRUPA006: hlavní 30° okap 4,0, západní křídlo okap 4,3 se šikmou stěnou 38° a štítovým schodem; štít hlavní střechy (3 m) přesah 1,0, okap štítu 6,0 (pod hřebenem 6,89)",
            [W(10, 0, Def(30, 0, 4.0)), W(10, 90, Def(90, 1.0, 4.0)), W(16, 180, Def(30, 0, 4.0)),
             W(5, 270, Def(90, 0, 4.3)), W(4, 0, Def(30, 0, 4.3)), W(2, 270, Def(38, 0, 4.3)),
             W(2, 0, Def(90, 0, 4.3)), W(3, 270, Def(90, 1.0, 6.0))]),
        "C69_raised_gable_level_eave": ("Jako C68, okap štítu 4,3 (stejně jako křídlo)",
            [W(10, 0, Def(30, 0, 4.0)), W(10, 90, Def(90, 1.0, 4.0)), W(16, 180, Def(30, 0, 4.0)),
             W(5, 270, Def(90, 0, 4.3)), W(4, 0, Def(30, 0, 4.3)), W(2, 270, Def(38, 0, 4.3)),
             W(2, 0, Def(90, 0, 4.3)), W(3, 270, Def(90, 1.0, 4.3))]),
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
    own = [d for d in dormers if d.roof is None]
    roofs = [d.roof if d.roof is not None else 2 + own.index(d) for d in dormers]
    out = [b"DACAD\x1a",
           _rec(0x0001, b"DACAD V1.13\0"),
           _rec(0x0002, b"$Header: tectora dacad_testcases.py $\0"),
           # Scale table copied from DACAD's own V1.13 samples.
           _rec(0x0003, struct.pack("<15H", 1, 1000, 2000, 3000, 12000, 5000,
                                    6000, 7000, 8000, 9000, 10000, 0, 0, 0, 0)),
           _rec(0x0004, bytes.fromhex("4111d42d")),
           _rec(0x0005, bytes.fromhex("7b247e2e")),
           _rec(0x0006, b"1.29H\0"),
           _rec(0x0010, struct.pack("<3H", 1 + len(own), n_walls, len(chimneys))),
           _rec(0x0011, struct.pack("<3H", 1, 0, 0))]
    for r, d in zip(roofs, dormers):
        if d.roof is None:
            out.append(_rec(0x0011, struct.pack("<3H", r, d.kind, d.host)))
    x = y = 0.0
    for i, w in enumerate(walls, start=1):
        out += _wall_rec(i, 1, x, y, w)
        x += w.length * math.cos(math.radians(w.deg))
        y += w.length * math.sin(math.radians(w.deg))
    if math.hypot(x, y) > 1e-6:
        raise ValueError(f"footprint not closed, ends at ({x:.3f}, {y:.3f})")
    i = len(walls)
    for r, d in zip(roofs, dormers):
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
