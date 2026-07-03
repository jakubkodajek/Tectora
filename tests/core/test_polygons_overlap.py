"""polygons_overlap: positive-area overlap detection for building footprints."""
from tectora.core.geom2d import (
    point_in_polygon,
    polygon_interior_point,
    polygons_overlap,
    segments_cross,
)

RECT = [(0, 0), (10, 0), (10, 6), (0, 6)]
L_SHAPE = [(0, 0), (6, 0), (6, 3), (3, 3), (3, 6), (0, 6)]


def test_disjoint_rects_do_not_overlap():
    other = [(20, 0), (26, 0), (26, 4), (20, 4)]
    assert not polygons_overlap(RECT, other)


def test_crossing_rects_overlap():
    other = [(5, 3), (15, 3), (15, 9), (5, 9)]
    assert polygons_overlap(RECT, other)
    assert polygons_overlap(other, RECT)


def test_nested_rects_overlap():
    inner = [(2, 2), (6, 2), (6, 5), (2, 5)]
    assert polygons_overlap(RECT, inner)
    assert polygons_overlap(inner, RECT)


def test_identical_rects_overlap():
    assert polygons_overlap(RECT, list(RECT))


def test_wall_to_wall_neighbours_do_not_overlap():
    # Shares the whole right edge of RECT — touching is not overlapping.
    neighbour = [(10, 0), (16, 0), (16, 6), (10, 6)]
    assert not polygons_overlap(RECT, neighbour)


def test_corner_touch_does_not_overlap():
    neighbour = [(10, 6), (14, 6), (14, 10), (10, 10)]
    assert not polygons_overlap(RECT, neighbour)


def test_rect_in_l_notch_does_not_overlap():
    # Fills the L's notch exactly: touches two edges, zero shared area.
    notch = [(3, 3), (6, 3), (6, 6), (3, 6)]
    assert not polygons_overlap(L_SHAPE, notch)


def test_rect_overlapping_l_wing_overlaps():
    box = [(2, 2), (5, 2), (5, 5), (2, 5)]   # crosses the notch boundary
    assert polygons_overlap(L_SHAPE, box)


def test_interior_point_is_strictly_inside():
    for poly in (RECT, L_SHAPE):
        p = polygon_interior_point(poly)
        assert point_in_polygon(p, poly)


def test_segments_cross_endpoint_touch_is_not_a_cross():
    assert segments_cross((0, 0), (4, 4), (0, 4), (4, 0))
    assert not segments_cross((0, 0), (4, 0), (4, 0), (8, 0))   # collinear chain
    assert not segments_cross((0, 0), (4, 0), (2, 0), (2, 4))   # T-touch
