import math
import pytest
from tectora.core.straight_skeleton import skeletonize


def _close(p, q, tol=1e-4):
    return abs(p[0] - q[0]) < tol and abs(p[1] - q[1]) < tol


def test_square_collapses_to_center():
    g = skeletonize([(0, 0), (8, 0), (8, 8), (0, 8)])
    # one interior node at the centre, height 4
    assert len(g.nodes) == 1
    cx, cy, t = g.nodes[0]
    assert (cx, cy) == pytest.approx((4.0, 4.0), abs=1e-4)
    assert t == pytest.approx(4.0, abs=1e-4)
    # four hip arcs from the corners
    assert len(g.arcs) == 4


def test_rectangle_has_ridge_of_two_nodes():
    g = skeletonize([(0, 0), (12, 0), (12, 8), (0, 8)])
    assert len(g.nodes) == 2
    pts = sorted((round(x, 4), round(y, 4)) for (x, y, _t) in g.nodes)
    assert pts[0] == pytest.approx((4.0, 4.0), abs=1e-3)
    assert pts[1] == pytest.approx((8.0, 4.0), abs=1e-3)
    assert all(abs(t - 4.0) < 1e-3 for (_x, _y, t) in g.nodes)
    # 4 hips + 1 ridge = 5 arcs
    assert len(g.arcs) == 5


def test_triangle_single_node():
    g = skeletonize([(0, 0), (6, 0), (3, 5)])
    assert len(g.nodes) == 1
    assert len(g.arcs) == 3


@pytest.mark.parametrize("poly", [
    [(0, 0), (10, 0), (10, 6), (0, 6)],
    [(0, 0), (5, 0), (7, 4), (3, 7), (-1, 4)],   # convex pentagon
])
def test_convex_terminates_nodes_inside(poly):
    g = skeletonize(poly)
    assert len(g.nodes) >= 1
    # every node lies within the polygon bounding box
    xs = [p[0] for p in poly]; ys = [p[1] for p in poly]
    for (x, y, _t) in g.nodes:
        assert min(xs) - 1e-6 <= x <= max(xs) + 1e-6
        assert min(ys) - 1e-6 <= y <= max(ys) + 1e-6
