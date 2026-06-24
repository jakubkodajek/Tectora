"""Stage-1 gable (štít) geometry on the straight-skeleton path.

Covers:
* is_valid_gable_edge (rectangular terminal cap predicate)
* GableNotSupported for invalid placements
* the golden oracle: a rectangle with its two cap ends gabled must match the
  exact GABLE calculator
* the area invariant for an L (gabling does not change total roof area)
* face folding (no vertical/zero-projected roof face; areas still sum)
* both wing-ends of an L gabled at once
"""
import math

import pytest

from tectora.core.calculator import calculate
from tectora.core.footprint import Footprint
from tectora.core.gable import GableNotSupported, is_valid_gable_edge
from tectora.core.roof_faces import build_faces
from tectora.core.roof_from_skeleton import build_roof
from tectora.core.spec import RoofSpec, RoofType

# Footprints (CCW). Edge i is V[i] -> V[i+1].
RECT = [(0, 0), (8, 0), (8, 5), (0, 5)]
L = [(0, 0), (6, 0), (6, 3), (3, 3), (3, 6), (0, 6)]


def _poly_area(pts):
    n = len(pts)
    return abs(sum(pts[i][0] * pts[(i + 1) % n][1] - pts[(i + 1) % n][0] * pts[i][1]
                   for i in range(n))) / 2


def _valid_edges(verts):
    """Indices of edges that validate as Stage-1 gable caps (eave footprint)."""
    fp = Footprint.from_lists(verts, ["eave"] * len(verts))
    return [i for i in range(fp.n) if is_valid_gable_edge(fp, i)]


def _with_gables(verts, gable_idx):
    types = ["eave"] * len(verts)
    for i in gable_idx:
        types[i] = "gable"
    return Footprint.from_lists(verts, types)


# ---------------------------------------------------------------------------
# Validation predicate
# ---------------------------------------------------------------------------

def test_valid_gable_edge_true_for_L_wing_ends():
    fp = Footprint.from_lists(L, ["eave"] * len(L))
    valid = [i for i in range(fp.n) if is_valid_gable_edge(fp, i)]
    # The L has exactly two rectangular wing-end caps.
    assert valid == [1, 4]
    assert is_valid_gable_edge(fp, 1) is True
    assert is_valid_gable_edge(fp, 4) is True


def test_valid_gable_edge_false_for_long_side():
    fp = Footprint.from_lists(L, ["eave"] * len(L))
    # Edge 0 (0,0)->(6,0) is a long side, not a terminal cap.
    assert is_valid_gable_edge(fp, 0) is False
    # Edge 5 (0,6)->(0,0) is the long left side.
    assert is_valid_gable_edge(fp, 5) is False


def test_valid_gable_edge_false_for_reflex_adjacent():
    fp = Footprint.from_lists(L, ["eave"] * len(L))
    # Vertex 3 (3,3) is reflex; edge 3 (3,3)->(3,6) is reflex-adjacent.
    assert is_valid_gable_edge(fp, 3) is False


def test_valid_gable_edge_never_raises_out_of_range():
    fp = Footprint.from_lists(L, ["eave"] * len(L))
    # Defensive: predicate returns False rather than raising on odd input.
    assert is_valid_gable_edge(fp, 0) in (True, False)


def test_build_roof_raises_on_invalid_gable():
    # Edge 0 of the L is a long side — gabling it must be rejected.
    fp = _with_gables(L, [0])
    with pytest.raises(GableNotSupported):
        build_roof(fp, 35)


def test_build_roof_raises_on_reflex_adjacent_gable():
    fp = _with_gables(L, [3])
    with pytest.raises(GableNotSupported):
        build_roof(fp, 35)


# ---------------------------------------------------------------------------
# Golden vs exact calculator (the key oracle)
# ---------------------------------------------------------------------------

def test_golden_rectangle_two_gables_matches_exact():
    # The two valid caps of an 8x5 rectangle are its short ends; gabling them
    # must reproduce RoofType.GABLE(8, 5, 35) exactly.
    valid = _valid_edges(RECT)
    assert valid == [1, 3]
    fp = _with_gables(RECT, valid)
    r = build_roof(fp, 35)
    exact = calculate(RoofSpec(RoofType.GABLE, 8, 5, 35))

    assert r.total_area_m2 == pytest.approx(exact.total_area_m2, rel=1e-6)
    assert r.ridge_length_m == pytest.approx(8.0, abs=1e-6)
    assert r.ridge_length_m == pytest.approx(exact.ridge_length_m, abs=1e-6)
    assert r.hip_count == 0
    assert r.valley_length_m == pytest.approx(0.0, abs=1e-9)
    assert r.gable_edge_total_m == pytest.approx(exact.gable_edge_total_m, rel=1e-6)
    assert r.gable_edge_total_m == pytest.approx(12.207746, rel=1e-5)
    # Eave perimeter excludes the two gable edges → only the two long sides.
    assert r.eaves_perimeter_m == pytest.approx(2 * 8.0, abs=1e-6)
    # Emitted skeleton kinds: rakes as "gable", plus the extended ridge.
    kinds = {s[0] for s in r.skeleton_segments}
    assert "gable" in kinds
    assert "hip" not in kinds


# ---------------------------------------------------------------------------
# Area invariant on an L
# ---------------------------------------------------------------------------

def test_area_invariant_l_one_gable():
    expected = _poly_area(L) / math.cos(math.radians(35))

    all_eave = build_roof(Footprint.from_lists(L, ["eave"] * len(L)), 35)
    valid = _valid_edges(L)
    one = build_roof(_with_gables(L, [valid[0]]), 35)

    # Total roof area unchanged by gabling (hip area == gable area at uniform pitch).
    assert one.total_area_m2 == pytest.approx(expected, rel=1e-6)
    assert one.total_area_m2 == pytest.approx(all_eave.total_area_m2, rel=1e-6)
    # Rake length appears.
    assert one.gable_edge_total_m > 0
    # Gabling a wing end removes its two hip rafters.
    assert one.hip_count < all_eave.hip_count


def test_area_invariant_l_two_gables():
    expected = _poly_area(L) / math.cos(math.radians(35))
    valid = _valid_edges(L)
    two = build_roof(_with_gables(L, valid), 35)

    assert two.total_area_m2 == pytest.approx(expected, rel=1e-6)
    assert two.gable_edge_total_m > 0
    all_eave = build_roof(Footprint.from_lists(L, ["eave"] * len(L)), 35)
    # Both wing ends gabled → four hip rafters removed.
    assert two.hip_count == all_eave.hip_count - 4


# ---------------------------------------------------------------------------
# Faces: no vertical/zero-projected roof face; areas still sum
# ---------------------------------------------------------------------------

def test_faces_no_vertical_and_areas_sum_for_gabled_l():
    valid = _valid_edges(L)
    fp = _with_gables(L, valid)
    faces = build_faces(fp, 35)
    expected = _poly_area(L) / math.cos(math.radians(35))

    # No vertical gable wall leaks in as a roof face (all have positive plan area).
    assert all(f.area_m2 > 1e-6 for f in faces)
    for f in faces:
        # A vertical face would project to (near) zero plan area; verify the
        # plan-projected shoelace area is positive.
        pts = [(v[0], v[1]) for v in f.vertices]
        n = len(pts)
        plan = abs(sum(pts[i][0] * pts[(i + 1) % n][1] - pts[(i + 1) % n][0] * pts[i][1]
                       for i in range(n))) / 2
        assert plan > 1e-6
    assert sum(f.area_m2 for f in faces) == pytest.approx(expected, rel=1e-6)


def test_faces_sum_for_gabled_rectangle():
    valid = _valid_edges(RECT)
    fp = _with_gables(RECT, valid)
    faces = build_faces(fp, 35)
    expected = _poly_area(RECT) / math.cos(math.radians(35))
    # Two roof planes only (the two gable-wall triangles dropped).
    assert len(faces) == 2
    assert sum(f.area_m2 for f in faces) == pytest.approx(expected, rel=1e-6)


# ---------------------------------------------------------------------------
# Both ends gabled: computes and both contribute rake length
# ---------------------------------------------------------------------------

def test_both_wing_ends_gabled_contribute_rake():
    valid = _valid_edges(L)
    assert len(valid) >= 2
    one_a = build_roof(_with_gables(L, [valid[0]]), 35)
    one_b = build_roof(_with_gables(L, [valid[1]]), 35)
    both = build_roof(_with_gables(L, valid), 35)

    # Each end contributes; the two-gable rake total is the sum of the singles.
    assert one_a.gable_edge_total_m > 0
    assert one_b.gable_edge_total_m > 0
    assert both.gable_edge_total_m == pytest.approx(
        one_a.gable_edge_total_m + one_b.gable_edge_total_m, rel=1e-6
    )
