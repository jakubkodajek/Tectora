import pytest
from tectora.core.straight_skeleton import build_lav


def square():
    return [(0, 0), (1, 0), (1, 1), (0, 1)]  # CCW


def l_shape():
    return [(0, 0), (6, 0), (6, 3), (3, 3), (3, 5), (0, 5)]  # CCW, reflex at (3,3)


def test_square_all_convex_bisectors_point_inward():
    lav = build_lav(square())
    verts = lav.vertices()
    assert len(verts) == 4
    assert all(not v.is_reflex for v in verts)
    v0 = next(v for v in verts if v.point == (0.0, 0.0))
    # bisector of corner (0,0) points toward centre -> (+,+), 45 degrees
    bx, by = v0.bisector
    assert bx == pytest.approx(2 ** -0.5, abs=1e-6)
    assert by == pytest.approx(2 ** -0.5, abs=1e-6)


def test_lshape_has_exactly_one_reflex():
    lav = build_lav(l_shape())
    verts = lav.vertices()
    assert sum(v.is_reflex for v in verts) == 1
    reflex = next(v for v in verts if v.is_reflex)
    assert reflex.point == (3.0, 3.0)


def test_lav_is_circular():
    lav = build_lav(square())
    v = lav.head
    seen = []
    for _ in range(4):
        seen.append(v.point)
        v = v.next
    assert v is lav.head            # wrapped around
    assert len(set(seen)) == 4
