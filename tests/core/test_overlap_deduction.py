# -*- coding: utf-8 -*-
"""Polygon intersection area + the overlap deduction of a covered roof.

When a higher building (tower/vestavba) stands over a lower one, the lower
roof under its footprint does not exist — the design total must subtract it,
face by face, slope-corrected.
"""
import math

from tectora.core.geom2d import clip_polygon_convex, intersection_area, polygon_area, triangulate


RECT = [(0, 0), (4, 0), (4, 2), (0, 2)]


# ---------------------------------------------------------------------------
# geom2d: intersection area
# ---------------------------------------------------------------------------

def test_polygon_area_shoelace():
    assert abs(polygon_area(RECT) - 8.0) < 1e-9
    assert abs(polygon_area(list(reversed(RECT))) - 8.0) < 1e-9


def test_triangulate_covers_the_polygon():
    lshape = [(0, 0), (6, 0), (6, 3), (3, 3), (3, 6), (0, 6)]
    tris = triangulate(lshape)
    assert len(tris) == len(lshape) - 2
    assert abs(sum(polygon_area(list(t)) for t in tris) - polygon_area(lshape)) < 1e-9


def test_clip_by_convex_triangle():
    tri = [(0, 0), (4, 0), (0, 4)]
    piece = clip_polygon_convex(RECT, tri)
    # Rectangle 4×2 clipped by x+y ≤ 4 → 8 minus the (2×2)/2 corner = 6.
    assert abs(polygon_area(piece) - 6.0) < 1e-9


def test_intersection_of_overlapping_rects():
    other = [(2, 1), (6, 1), (6, 5), (2, 5)]
    assert abs(intersection_area(RECT, other) - 2.0) < 1e-9        # 2×1
    assert abs(intersection_area(other, RECT) - 2.0) < 1e-9        # symmetric


def test_intersection_disjoint_and_contained():
    assert intersection_area(RECT, [(10, 10), (12, 10), (12, 12), (10, 12)]) == 0.0
    inner = [(1, 0.5), (2, 0.5), (2, 1.5), (1, 1.5)]
    assert abs(intersection_area(RECT, inner) - 1.0) < 1e-9
    assert abs(intersection_area(inner, RECT) - 1.0) < 1e-9


def test_intersection_with_nonconvex_polygon():
    lshape = [(0, 0), (6, 0), (6, 3), (3, 3), (3, 6), (0, 6)]
    big = [(0, 0), (6, 0), (6, 6), (0, 6)]
    assert abs(intersection_area(big, lshape) - polygon_area(lshape)) < 1e-9
    assert abs(intersection_area(lshape, big) - polygon_area(lshape)) < 1e-9


# ---------------------------------------------------------------------------
# use_cases: overlap deduction + design total
# ---------------------------------------------------------------------------

def _building(pts, pitch=35.0, eave=2.5, name=""):
    from tectora.application.project import Building
    from tectora.core.footprint import Footprint
    return Building(
        footprint=Footprint.from_lists(list(pts), ["eave"] * len(pts)),
        pitch_deg=pitch, eave_height_m=eave, name=name,
    )


def test_tower_deducts_covered_roof_slope_corrected():
    """Tower 4×2 fully inside ONE face of the big hip roof → the deduction is
    exactly tower plan area / cos(pitch)."""
    from tectora.application.project import Project
    from tectora.application import use_cases as uc

    big = _building([(0, 0), (20, 0), (20, 10), (0, 10)], pitch=35.0, eave=2.5)
    tower = _building([(4, 1), (8, 1), (8, 3), (4, 3)], pitch=35.0, eave=6.0)
    design = uc.calculate_design(Project(project_name="X", buildings=[big, tower]))

    expected = 8.0 / math.cos(math.radians(35.0))
    assert abs(design.overlap_deduction_m2 - expected) < 1e-6
    per_building = sum(r.total_area_m2 for r in design.buildings)
    assert abs(design.total_area_m2 - (per_building - expected)) < 1e-6


def test_disjoint_buildings_have_no_deduction():
    from tectora.application.project import Project
    from tectora.application import use_cases as uc

    a = _building([(0, 0), (10, 0), (10, 6), (0, 6)])
    b = _building([(20, 0), (26, 0), (26, 4), (20, 4)])
    design = uc.calculate_design(Project(project_name="X", buildings=[a, b]))
    assert design.overlap_deduction_m2 == 0.0


def test_lower_building_keeps_its_roof():
    """The deduction is taken from the LOWER roof only — swapping the eave
    heights moves the deduction to the other building's faces (here the
    tower shape differs, so the amounts differ)."""
    from tectora.application.project import Project
    from tectora.application import use_cases as uc

    big = _building([(0, 0), (20, 0), (20, 10), (0, 10)], eave=6.0)   # higher
    small = _building([(4, 1), (8, 1), (8, 3), (4, 3)], eave=2.5)     # lower
    design = uc.calculate_design(Project(project_name="X", buildings=[big, small]))
    # The small building is fully covered → its ENTIRE roof is deducted.
    small_result = design.buildings[1]
    assert abs(design.overlap_deduction_m2 - small_result.total_area_m2) < 1e-6
