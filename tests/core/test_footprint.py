import pytest
from tectora.core.footprint import Edge, EdgeType, Footprint


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
