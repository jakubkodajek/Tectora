# -*- coding: utf-8 -*-
"""generate_roof emits exact-calculator results in MODEL coordinates.

The bridge picks the calculator-frame placement from the edge types (ridge
parallel to the eaves, shed low side on its single eave), and generate_roof
maps faces + interior segments through it. Consumers (2D canvas, 3D view)
then share one geometry — the historic source of 2D-vs-3D mismatches — and
gables work on ANY side of a rectangle, not just the shorter one.
"""
import math

from tectora.core.footprint import Footprint
from tectora.core.roof_generator import generate_roof


def _fp(pts, types):
    return Footprint.from_lists(list(pts), list(types))


def _rect(x0, y0, x1, y1, types):
    return _fp([(x0, y0), (x1, y0), (x1, y1), (x0, y1)], types)


def _kinds(result):
    return [s[0] for s in result.skeleton_segments]


def _seg3d_len(s):
    (_k, a, b) = s
    return math.dist(a, b)


def _all_xy(result):
    pts = [(v[0], v[1]) for f in result.faces for v in f.vertices]
    pts += [(p[0], p[1]) for (_k, a, b) in result.skeleton_segments for p in (a, b)]
    return pts


def _assert_inside(pts, x0, y0, x1, y1, eps=1e-6):
    assert pts
    for (x, y) in pts:
        assert x0 - eps <= x <= x1 + eps and y0 - eps <= y <= y1 + eps, (
            f"({x}, {y}) mimo půdorys ({x0},{y0})–({x1},{y1})")


# ---------------------------------------------------------------------------
# Segment classification parity with the old exact-path rendering
# ---------------------------------------------------------------------------

def test_gable_rect_emits_one_ridge_no_hips():
    r = generate_roof(_rect(0, 0, 12, 8, ["eave", "gable", "eave", "gable"]), 35)
    k = _kinds(r)
    assert k.count("ridge") == 1 and k.count("hip") == 0


def test_hip_rect_emits_ridge_and_four_hips():
    r = generate_roof(_rect(0, 0, 12, 8, ["eave"] * 4), 35)
    k = _kinds(r)
    assert k.count("ridge") == 1 and k.count("hip") == 4
    hips = [s for s in r.skeleton_segments if s[0] == "hip"]
    per = r.hip_length_m / r.hip_count
    assert all(abs(_seg3d_len(s) - per) < 1e-6 for s in hips)
    ridge = [s for s in r.skeleton_segments if s[0] == "ridge"][0]
    assert abs(_seg3d_len(ridge) - r.ridge_length_m) < 1e-6


def test_pyramid_emits_four_hips_no_ridge():
    r = generate_roof(_rect(0, 0, 8, 8, ["eave"] * 4), 35)
    k = _kinds(r)
    assert k.count("ridge") == 0 and k.count("hip") == 4


# ---------------------------------------------------------------------------
# Model placement: results land on the drawn rectangle
# ---------------------------------------------------------------------------

def test_offset_rectangle_results_are_in_model_frame():
    r = generate_roof(_rect(20, 10, 32, 18, ["eave"] * 4), 35)
    _assert_inside(_all_xy(r), 20, 10, 32, 18)
    ridge = [s for s in r.skeleton_segments if s[0] == "ridge"][0]
    (_k, a, b) = ridge
    assert abs(a[1] - 14.0) < 1e-6 and abs(b[1] - 14.0) < 1e-6   # centreline


def test_portrait_rectangle_ridge_runs_along_y():
    r = generate_roof(_rect(0, 0, 6, 16, ["eave"] * 4), 35)
    _assert_inside(_all_xy(r), 0, 0, 6, 16)
    ridge = [s for s in r.skeleton_segments if s[0] == "ridge"][0]
    (_k, a, b) = ridge
    assert abs(a[0] - 3.0) < 1e-6 and abs(b[0] - 3.0) < 1e-6     # x = centre
    assert abs(abs(b[1] - a[1]) - r.ridge_length_m) < 1e-6


# ---------------------------------------------------------------------------
# Gable on the LONGER side of a rectangle (ridge rotates with the eaves)
# ---------------------------------------------------------------------------

def test_gable_on_long_edges_rotates_the_ridge():
    # 12 × 8 rectangle, gables on the two LONG (horizontal) edges → the eaves
    # are the short vertical edges, so the ridge runs vertically at x = 6.
    r = generate_roof(_rect(0, 0, 12, 8, ["gable", "eave", "gable", "eave"]), 35)
    assert abs(r.ridge_length_m - 8.0) < 1e-6
    assert abs(r.eaves_perimeter_m - 16.0) < 1e-6      # 2 × the short side
    ridge = [s for s in r.skeleton_segments if s[0] == "ridge"][0]
    (_k, a, b) = ridge
    assert abs(a[0] - 6.0) < 1e-6 and abs(b[0] - 6.0) < 1e-6
    assert {round(a[1], 6), round(b[1], 6)} == {0.0, 8.0}
    # Apex height follows the 12 m span: (12/2)·tan(35°).
    assert abs(r.apex_height_m - 6.0 * math.tan(math.radians(35))) < 1e-6
    _assert_inside(_all_xy(r), 0, 0, 12, 8)


def test_gable_short_vs_long_total_areas_differ_consistently():
    short = generate_roof(_rect(0, 0, 12, 8, ["eave", "gable", "eave", "gable"]), 35)
    long_ = generate_roof(_rect(0, 0, 12, 8, ["gable", "eave", "gable", "eave"]), 35)
    # Both cover the same plan area at the same pitch → equal roof area.
    assert abs(short.total_area_m2 - long_.total_area_m2) < 1e-6
    assert abs(short.ridge_length_m - 12.0) < 1e-6
    assert abs(long_.ridge_length_m - 8.0) < 1e-6


# ---------------------------------------------------------------------------
# Shed orientation: the single eave edge is the LOW side
# ---------------------------------------------------------------------------

def _low_high_xy(result):
    """(low-edge points, high-edge points) of the single shed face."""
    face = result.faces[0]
    lows = [v for v in face.vertices if abs(v[2]) < 1e-9]
    highs = [v for v in face.vertices if v[2] > 1e-9]
    return lows, highs


def test_shed_low_side_south():
    r = generate_roof(_rect(0, 0, 12, 8, ["eave", "gable", "gable", "gable"]), 30)
    lows, highs = _low_high_xy(r)
    assert all(abs(v[1] - 0.0) < 1e-6 for v in lows)
    assert all(abs(v[1] - 8.0) < 1e-6 for v in highs)


def test_shed_low_side_north():
    r = generate_roof(_rect(0, 0, 12, 8, ["gable", "gable", "eave", "gable"]), 30)
    lows, highs = _low_high_xy(r)
    assert all(abs(v[1] - 8.0) < 1e-6 for v in lows)
    assert all(abs(v[1] - 0.0) < 1e-6 for v in highs)


def test_shed_low_side_west():
    r = generate_roof(_rect(0, 0, 12, 8, ["gable", "gable", "gable", "eave"]), 30)
    lows, highs = _low_high_xy(r)
    assert all(abs(v[0] - 0.0) < 1e-6 for v in lows)
    assert all(abs(v[0] - 12.0) < 1e-6 for v in highs)


def test_shed_low_side_east():
    r = generate_roof(_rect(0, 0, 12, 8, ["gable", "eave", "gable", "gable"]), 30)
    lows, highs = _low_high_xy(r)
    assert all(abs(v[0] - 12.0) < 1e-6 for v in lows)
    assert all(abs(v[0] - 0.0) < 1e-6 for v in highs)


# ---------------------------------------------------------------------------
# Asymmetric gable: the front face is the deterministic south/west one
# ---------------------------------------------------------------------------

def test_asymmetric_gable_front_is_south_face():
    r = generate_roof(_rect(0, 0, 12, 8, ["eave", "gable", "eave", "gable"]), 40,
                      pitch_back=20)
    front = next(f for f in r.faces if "přední" in f.name)
    back = next(f for f in r.faces if "zadní" in f.name)
    assert abs(front.slope_deg - 40) < 1e-6
    assert abs(back.slope_deg - 20) < 1e-6
    # front face touches y = 0, back face touches y = 8
    assert any(abs(v[1]) < 1e-6 for v in front.vertices)
    assert any(abs(v[1] - 8.0) < 1e-6 for v in back.vertices)


def _newell_nz(vertices):
    nz = 0.0
    m = len(vertices)
    for i in range(m):
        ax, ay, _az = vertices[i]
        bx, by, _bz = vertices[(i + 1) % m]
        nz += (ax - bx) * (ay + by)
    return nz


def test_every_exact_face_has_an_upward_normal():
    """The 3D view shades by the face normal — every exact-path face must
    come out CCW in plan (normal up), whatever the calculator emitted and
    whichever (possibly reflected) frame placed it."""
    cases = [
        (["eave"] * 4, {}),                                   # hip
        (["eave", "gable", "eave", "gable"], {}),             # gable, short caps
        (["gable", "eave", "gable", "eave"], {}),             # gable, long sides
        (["eave", "gable", "eave", "gable"], {"pitch_back": 20}),
        (["eave", "gable", "eave", "gable"], {"half_hip_m": 1.0}),
        (["eave", "gable", "gable", "gable"], {}),            # shed south
        (["gable", "gable", "eave", "gable"], {}),            # shed north
        (["gable", "gable", "gable", "eave"], {}),            # shed west
    ]
    for types, kw in cases:
        r = generate_roof(_rect(0, 0, 12, 8, types), 35, **kw)
        for f in r.faces:
            assert _newell_nz(f.vertices) > 0, (types, kw, f.name)
    # Square pyramid too.
    r = generate_roof(_rect(0, 0, 8, 8, ["eave"] * 4), 35)
    for f in r.faces:
        assert _newell_nz(f.vertices) > 0, ("pyramid", f.name)


def test_half_hip_lands_on_offset_footprint():
    r = generate_roof(_rect(-14, 6, -4, 12, ["eave", "gable", "eave", "gable"]), 35,
                      half_hip_m=1.0)
    _assert_inside(_all_xy(r), -14, 6, -4, 12)
    assert r.spec.type.value == "half_hip"
