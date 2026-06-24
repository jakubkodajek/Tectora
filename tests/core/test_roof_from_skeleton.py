"""Tests for roof_from_skeleton.build_roof — Part C (SS4)."""
import math
import pytest
from tectora.core.calculator import calculate
from tectora.core.footprint import Footprint
from tectora.core.roof_from_skeleton import build_roof
from tectora.core.spec import RoofSpec, RoofType


def rect(w, h):
    return Footprint.from_lists([(0, 0), (w, 0), (w, h), (0, h)], ["eave"] * 4)


def l_shape():
    return Footprint.from_lists(
        [(0, 0), (6, 0), (6, 3), (3, 3), (3, 5), (0, 5)], ["eave"] * 6)


def _poly_area(pts):
    n = len(pts)
    return abs(sum(pts[i][0]*pts[(i+1) % n][1] - pts[(i+1) % n][0]*pts[i][1]
                   for i in range(n))) / 2


def test_rectangle_matches_exact_calculator():
    r = build_roof(rect(12, 8), 35)
    exact = calculate(RoofSpec(RoofType.HIP, 12, 8, 35))
    assert r.total_area_m2 == pytest.approx(exact.total_area_m2, rel=1e-3)
    assert r.ridge_length_m == pytest.approx(exact.ridge_length_m, abs=1e-2)
    assert r.hip_count == 4
    assert r.hip_length_m == pytest.approx(exact.hip_length_m, rel=1e-2)
    assert r.valley_length_m == pytest.approx(0.0, abs=1e-6)


def test_square_matches_pyramid():
    r = build_roof(rect(8, 8), 35)
    exact = calculate(RoofSpec(RoofType.PYRAMID, 8, 8, 35))
    assert r.total_area_m2 == pytest.approx(exact.total_area_m2, rel=1e-3)
    assert r.hip_count == 4
    assert r.ridge_length_m == pytest.approx(0.0, abs=1e-2)


def test_area_invariant_l_shape():
    fp = l_shape()
    r = build_roof(fp, 35)
    expected = _poly_area(list(fp.vertices)) / math.cos(math.radians(35))
    assert r.total_area_m2 == pytest.approx(expected, rel=1e-6)


def test_l_shape_has_valley():
    r = build_roof(l_shape(), 35)
    assert r.valley_count >= 1
    assert r.valley_length_m > 0
    assert any(s[0] == "valley" for s in r.skeleton_segments)
