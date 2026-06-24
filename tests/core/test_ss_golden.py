"""Golden geometric oracles for the straight skeleton.

These pin the ACTUAL geometry (node positions, ridge length), which the earlier
structural SS3 tests did not — that gap let a broken reflex skeleton pass.

L-shape (hand-derived): left column 3x5 + right square 3x3 sharing the reflex
corner (3,3). Skeleton: three nodes all at offset t=1.5; two ridges (vertical
2 m + horizontal 3 m = 5 m); one valley; five hips.
"""
import math

import pytest

from tectora.core.calculator import calculate
from tectora.core.footprint import Footprint
from tectora.core.roof_from_skeleton import build_roof
from tectora.core.spec import RoofSpec, RoofType
from tectora.core.straight_skeleton import skeletonize

L = [(0, 0), (6, 0), (6, 3), (3, 3), (3, 5), (0, 5)]  # CCW, reflex at (3,3)
U = [(0, 0), (7, 0), (7, 5), (5, 5), (5, 2), (2, 2), (2, 5), (0, 5)]  # notch


def _has_node(graph, x, y, t, tol=1e-2):
    return any(abs(n[0] - x) < tol and abs(n[1] - y) < tol and abs(n[2] - t) < tol
               for n in graph.nodes)


def _area_over_cos(poly, pitch):
    n = len(poly)
    a = abs(sum(poly[i][0] * poly[(i + 1) % n][1] - poly[(i + 1) % n][0] * poly[i][1]
                for i in range(n))) / 2
    return a / math.cos(math.radians(pitch))


def test_lshape_golden_node_positions():
    g = skeletonize(L)
    assert _has_node(g, 1.5, 1.5, 1.5), g.nodes
    assert _has_node(g, 1.5, 3.5, 1.5), g.nodes
    assert _has_node(g, 4.5, 1.5, 1.5), g.nodes
    assert len(g.nodes) == 3, g.nodes


def test_lshape_golden_roof_measures():
    r = build_roof(Footprint.from_lists(L, ["eave"] * 6), 35)
    assert r.ridge_length_m == pytest.approx(5.0, abs=1e-2)  # 2 m + 3 m
    assert r.valley_count == 1
    assert r.hip_count == 5
    assert r.total_area_m2 == pytest.approx(_area_over_cos(L, 35), rel=1e-6)


def test_rectangle_crosscheck_via_skeleton():
    fp = Footprint.from_lists([(0, 0), (12, 0), (12, 8), (0, 8)], ["eave"] * 4)
    r = build_roof(fp, 35)
    e = calculate(RoofSpec(RoofType.HIP, 12, 8, 35))
    assert r.ridge_length_m == pytest.approx(e.ridge_length_m, abs=1e-3)
    assert r.hip_length_m == pytest.approx(e.hip_length_m, rel=1e-3)
    assert r.total_area_m2 == pytest.approx(e.total_area_m2, rel=1e-6)


@pytest.mark.parametrize("poly", [L, U])
def test_area_invariant_and_valley_present(poly):
    r = build_roof(Footprint.from_lists(poly, ["eave"] * len(poly)), 35)
    assert r.total_area_m2 == pytest.approx(_area_over_cos(poly, 35), rel=1e-6)
    assert r.valley_count >= 1
