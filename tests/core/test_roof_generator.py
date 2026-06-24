import math
import pytest
from tectora.core.calculator import calculate
from tectora.core.footprint import Footprint
from tectora.core.roof_generator import generate_roof
from tectora.core.spec import RoofSpec, RoofType


def test_rectangle_uses_exact_calculator_with_faces():
    fp = Footprint.from_lists([(0, 0), (12, 0), (12, 8), (0, 8)], ["eave"] * 4)
    r = generate_roof(fp, 35)
    exact = calculate(RoofSpec(RoofType.HIP, 12, 8, 35))
    assert r.total_area_m2 == pytest.approx(exact.total_area_m2, rel=1e-6)
    assert len(r.faces) == 4            # exact path gives per-face polygons


def test_lshape_uses_skeleton_with_valley():
    fp = Footprint.from_lists(
        [(0, 0), (6, 0), (6, 3), (3, 3), (3, 5), (0, 5)], ["eave"] * 6)
    r = generate_roof(fp, 35)
    assert r.valley_count >= 1
    assert r.valley_length_m > 0
    assert any(s[0] == "valley" for s in r.skeleton_segments)


def test_convex_pentagon_generates_roof():
    fp = Footprint.from_lists(
        [(0, 0), (6, 0), (7, 4), (3, 7), (-1, 4)], ["eave"] * 5)
    r = generate_roof(fp, 35)
    assert r.total_area_m2 > 0
    assert r.hip_count >= 1
    assert r.valley_count == 0
