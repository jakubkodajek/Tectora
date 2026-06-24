import math
import pytest
from tectora.core import geom2d as g


def test_basic_vector_ops():
    assert g.sub((3, 5), (1, 2)) == (2, 3)
    assert g.add((1, 2), (3, 4)) == (4, 6)
    assert g.scale((2, 3), 2) == (4, 6)
    assert g.dot((1, 0), (0, 1)) == 0
    assert g.cross((1, 0), (0, 1)) == 1
    assert g.length((3, 4)) == pytest.approx(5.0)
    nx, ny = g.normalize((3, 4))
    assert (nx, ny) == pytest.approx((0.6, 0.8))


def test_orientation():
    assert g.orientation((0, 0), (1, 0), (0, 1)) > 0   # left turn (CCW)
    assert g.orientation((0, 0), (1, 0), (0, -1)) < 0  # right turn
    assert g.orientation((0, 0), (1, 0), (2, 0)) == 0  # collinear


def test_line_intersection():
    p = g.line_intersection((0, 0), (1, 0), (1, -1), (0, 1))  # x-axis meets vertical x=1
    assert p == pytest.approx((1.0, 0.0))
    assert g.line_intersection((0, 0), (1, 0), (0, 1), (1, 0)) is None  # parallel


def test_dist_point_to_line():
    assert g.dist_point_to_line((0, 3), (0, 0), (1, 0)) == pytest.approx(3.0)
