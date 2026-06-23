"""Most: rectangle Footprint + typy hran → RoofSpec (reuse výpočtu)."""
from __future__ import annotations

from tectora.core.footprint import EdgeType, Footprint
from tectora.core.spec import RoofSpec, RoofType

_TOL = 1e-6


class UnsupportedFootprint(ValueError):
    """Tvar, který most na RoofSpec zatím neumí (obecný polygon → skeleton)."""


def _axis_aligned_rectangle(fp: Footprint) -> bool:
    if fp.n != 4:
        return False
    xs = sorted({round(v[0], 9) for v in fp.vertices})
    ys = sorted({round(v[1], 9) for v in fp.vertices})
    if len(xs) != 2 or len(ys) != 2:
        return False
    want = {(xs[0], ys[0]), (xs[1], ys[0]), (xs[1], ys[1]), (xs[0], ys[1])}
    return want == {(round(x, 9), round(y, 9)) for x, y in fp.vertices}


def footprint_to_spec(fp: Footprint, pitch_deg: float, name: str = "") -> RoofSpec:
    if not _axis_aligned_rectangle(fp):
        raise UnsupportedFootprint(
            "Most umí zatím jen obdélník; obecné polygony přijdou se skeletonem.")
    xs = sorted({v[0] for v in fp.vertices})
    ys = sorted({v[1] for v in fp.vertices})
    w, d = xs[1] - xs[0], ys[1] - ys[0]
    length, width = max(w, d), min(w, d)
    eaves = [i for i, e in enumerate(fp.edges) if e.type is EdgeType.EAVE]
    n = len(eaves)

    if n == 1:
        rtype = RoofType.SHED
    elif n == 2 and (eaves[1] - eaves[0]) == 2:
        rtype = RoofType.GABLE
    elif n == 4 and abs(w - d) < _TOL:
        rtype = RoofType.PYRAMID
    elif n == 4:
        rtype = RoofType.HIP
    else:
        raise UnsupportedFootprint(
            f"Kombinace {n} okapů na obdélníku není podporovaný typ.")

    return RoofSpec(type=rtype, length=length, width=width, pitch=pitch_deg, name=name)
