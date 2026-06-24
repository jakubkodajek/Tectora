import pytest
from tectora.core.footprint import (
    Edge,
    EdgeType,
    Footprint,
    drop_collinear_vertices,
    is_simple_polygon,
)


def rect(types):
    return Footprint.from_lists([(0, 0), (12, 0), (12, 8), (0, 8)], types)


def test_from_lists_and_lengths():
    fp = rect(["eave", "gable", "eave", "gable"])
    assert fp.n == 4
    assert fp.edges[1].type is EdgeType.GABLE
    assert fp.edge_length(0) == pytest.approx(12.0)
    assert fp.edge_length(1) == pytest.approx(8.0)


def test_too_few_vertices():
    with pytest.raises(ValueError):
        Footprint.from_lists([(0, 0), (1, 1)], ["eave", "eave"])


def test_edge_count_mismatch():
    with pytest.raises(ValueError):
        Footprint.from_lists([(0, 0), (4, 0), (4, 4), (0, 4)], ["eave", "eave"])


def test_self_intersection_rejected():
    with pytest.raises(ValueError):
        Footprint.from_lists([(0, 0), (4, 4), (4, 0), (0, 4)], ["eave"] * 4)


def test_bad_edge_type():
    with pytest.raises(ValueError):
        rect(["eave", "wall", "eave", "gable"])


def test_is_simple_polygon_true_for_rectangle():
    # CCW rectangle is a simple polygon
    assert is_simple_polygon([(0, 0), (12, 0), (12, 8), (0, 8)]) is True


def test_is_simple_polygon_false_for_bowtie():
    # Self-intersecting "bowtie" quad
    assert is_simple_polygon([(0, 0), (2, 2), (2, 0), (0, 2)]) is False


def test_drop_collinear_vertices_removes_midedge_points():
    # Rectangle split with a collinear point on the bottom edge and the right edge
    verts = [(0, 0), (3, 0), (6, 0), (6, 2), (6, 4), (0, 4)]
    types = ["eave"] * 6
    sv, se = drop_collinear_vertices(verts, types)
    assert sv == [(0, 0), (6, 0), (6, 4), (0, 4)]
    assert len(se) == 4


def test_drop_collinear_keeps_real_corners():
    # An L-shape has no collinear points → unchanged
    L = [(0, 0), (6, 0), (6, 3), (3, 3), (3, 6), (0, 6)]
    sv, se = drop_collinear_vertices(L, ["eave"] * 6)
    assert sv == L
    assert len(se) == 6


def test_drop_collinear_noop_for_triangle():
    tri = [(0, 0), (4, 0), (2, 3)]
    sv, se = drop_collinear_vertices(tri, ["eave"] * 3)
    assert sv == tri and len(se) == 3
