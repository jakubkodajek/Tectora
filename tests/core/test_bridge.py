import pytest
from tectora.core.bridge import footprint_to_spec, UnsupportedFootprint
from tectora.core.calculator import calculate
from tectora.core.footprint import Footprint
from tectora.core.spec import RoofType


def rect(w, h, types):
    return Footprint.from_lists([(0, 0), (w, 0), (w, h), (0, h)], types)


def test_gable_from_two_opposite_eaves():
    spec = footprint_to_spec(rect(12, 8, ["eave", "gable", "eave", "gable"]), 35)
    assert spec.type is RoofType.GABLE
    assert spec.length == pytest.approx(12)
    assert spec.width == pytest.approx(8)
    assert calculate(spec).ridge_length_m == pytest.approx(12)


def test_hip_from_four_eaves():
    spec = footprint_to_spec(rect(12, 8, ["eave"] * 4), 35)
    assert spec.type is RoofType.HIP


def test_pyramid_from_square_four_eaves():
    spec = footprint_to_spec(rect(8, 8, ["eave"] * 4), 35)
    assert spec.type is RoofType.PYRAMID


def test_shed_from_one_eave():
    spec = footprint_to_spec(rect(12, 8, ["eave", "gable", "gable", "gable"]), 35)
    assert spec.type is RoofType.SHED


def test_non_rectangle_unsupported():
    fp = Footprint.from_lists(
        [(0, 0), (10, 0), (10, 6), (5, 6), (5, 10), (0, 10)], ["eave"] * 6)
    with pytest.raises(UnsupportedFootprint):
        footprint_to_spec(fp, 35)
