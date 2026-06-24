"""Broad structural correctness of roof generation across footprint shapes.

For a full-hip roof the straight skeleton must satisfy: one hip rafter per convex
corner, one valley per reflex corner, ridges present whenever there are reflex
corners, and total area == footprint_area / cos(pitch). These hold for any simple
polygon, axis-aligned or sloped — this is what makes it "work for almost
everything".
"""
import math

import pytest

from tectora.core.footprint import Footprint
from tectora.core.roof_from_skeleton import build_roof
from tectora.core.straight_skeleton import build_lav

SHAPES = {
    "L": [(0, 0), (6, 0), (6, 3), (3, 3), (3, 5), (0, 5)],
    "U": [(0, 0), (7, 0), (7, 5), (5, 5), (5, 2), (2, 2), (2, 5), (0, 5)],
    "T": [(0, 4), (3, 4), (3, 0), (6, 0), (6, 4), (9, 4), (9, 7), (0, 7)],
    "step": [(0, 0), (4, 0), (4, 2), (8, 2), (8, 6), (0, 6)],
    "plus": [(2, 0), (4, 0), (4, 2), (6, 2), (6, 4), (4, 4),
             (4, 6), (2, 6), (2, 4), (0, 4), (0, 2), (2, 2)],
    "sloped": [(0, 0), (6, 0), (6, 3), (4, 3), (2, 6), (0, 6)],
}


def _convex_reflex(poly):
    nref = sum(v.is_reflex for v in build_lav(poly).vertices())
    return len(poly) - nref, nref


def _footprint_area(poly):
    n = len(poly)
    return abs(sum(poly[i][0] * poly[(i + 1) % n][1] - poly[(i + 1) % n][0] * poly[i][1]
                   for i in range(n))) / 2


@pytest.mark.parametrize("name", list(SHAPES))
def test_shape_structural_correctness(name):
    poly = SHAPES[name]
    n_convex, n_reflex = _convex_reflex(poly)
    r = build_roof(Footprint.from_lists(poly, ["eave"] * len(poly)), 35)

    assert r.hip_count == n_convex, name
    assert r.valley_count == n_reflex, name
    assert r.total_area_m2 == pytest.approx(
        _footprint_area(poly) / math.cos(math.radians(35)), rel=1e-6)
    if n_reflex > 0:
        assert r.ridge_length_m > 0, name
        assert any(s[0] == "valley" for s in r.skeleton_segments), name
