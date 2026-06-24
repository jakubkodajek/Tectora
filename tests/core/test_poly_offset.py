import pytest
from tectora.core.poly_offset import offset_polygon


def test_zero_offset_identity():
    sq = [(0, 0), (10, 0), (10, 10), (0, 10)]
    assert offset_polygon(sq, 0.0) == [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]


def test_square_offset_outward():
    sq = [(0, 0), (10, 0), (10, 10), (0, 10)]
    out = offset_polygon(sq, 1.0)
    assert out[0] == pytest.approx((-1.0, -1.0))
    assert out[1] == pytest.approx((11.0, -1.0))
    assert out[2] == pytest.approx((11.0, 11.0))
    assert out[3] == pytest.approx((-1.0, 11.0))
