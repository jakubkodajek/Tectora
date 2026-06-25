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


def test_collinear_split_rectangle_still_calculates():
    # A rectangle drawn with extra mid-edge points (collinear) must NOT be
    # rejected — it should calculate identically to the clean rectangle.
    split = Footprint.from_lists(
        [(0, 0), (3, 0), (6, 0), (6, 2), (6, 4), (0, 4)], ["eave"] * 6)
    clean = Footprint.from_lists([(0, 0), (6, 0), (6, 4), (0, 4)], ["eave"] * 4)
    r_split = generate_roof(split, 35)
    r_clean = generate_roof(clean, 35)
    assert r_split.total_area_m2 == pytest.approx(r_clean.total_area_m2, rel=1e-6)


def test_collinear_split_rectangle_with_overhang():
    split = Footprint.from_lists(
        [(0, 0), (3, 0), (6, 0), (6, 2), (6, 4), (0, 4)], ["eave"] * 6)
    r = generate_roof(split, 35, overhang=1.0)
    assert r.total_area_m2 > 0


def test_half_hip_clips_a_gable_rectangle():
    # Gables on the two short ends → a gable roof; a half-hip depth clips the
    # ends into a half-hip (polovalba) — ridge shorter than the gable's, 4 hips.
    types = ["eave", "gable", "eave", "gable"]
    rect = lambda: Footprint.from_lists([(0, 0), (10, 0), (10, 6), (0, 6)], types)
    g = generate_roof(rect(), 35, half_hip_m=0.0)
    hh = generate_roof(rect(), 35, half_hip_m=1.0)
    assert g.spec.type == RoofType.GABLE
    assert hh.spec.type == RoofType.HALF_HIP
    assert 0 < hh.ridge_length_m < g.ridge_length_m
    assert hh.hip_count == 4


def test_half_hip_ignored_without_gable():
    # No gable edges → a half-hip depth has no effect (stays a full hip).
    fp = Footprint.from_lists([(0, 0), (10, 0), (10, 6), (0, 6)], ["eave"] * 4)
    r = generate_roof(fp, 35, half_hip_m=1.0)
    assert r.spec.type == RoofType.HIP
