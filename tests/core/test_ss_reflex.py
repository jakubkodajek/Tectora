import pytest
from tectora.core.straight_skeleton import build_lav, skeletonize


def l_shape():
    return [(0, 0), (6, 0), (6, 3), (3, 3), (3, 5), (0, 5)]  # CCW, reflex at (3,3)


def test_reflex_bisector_points_inward():
    lav = build_lav(l_shape())
    v = next(v for v in lav.vertices() if v.point == (3.0, 3.0))
    assert v.is_reflex
    bx, by = v.bisector
    assert bx < 0 and by < 0          # interior is lower-left of the notch


def test_lshape_skeletonizes_and_terminates():
    g = skeletonize(l_shape())
    assert len(g.nodes) >= 2
    assert len(g.arcs) >= 6
    # the reflex corner is an endpoint of some arc going inward
    touches = [a for a in g.arcs if a[0] == (3.0, 3.0) or a[1] == (3.0, 3.0)]
    assert len(touches) >= 1


def test_lshape_nodes_inside_bbox():
    g = skeletonize(l_shape())
    for (x, y, _t) in g.nodes:
        assert -1e-6 <= x <= 6 + 1e-6
        assert -1e-6 <= y <= 5 + 1e-6


def test_plus_shape_terminates():
    plus = [(2, 0), (4, 0), (4, 2), (6, 2), (6, 4), (4, 4),
            (4, 6), (2, 6), (2, 4), (0, 4), (0, 2), (2, 2)]  # CCW plus, 4 reflex
    g = skeletonize(plus)
    assert len(g.nodes) >= 1   # must not raise / loop forever


def test_convex_still_works():
    g = skeletonize([(0, 0), (8, 0), (8, 8), (0, 8)])
    assert len(g.nodes) == 1   # SS2 behaviour preserved
