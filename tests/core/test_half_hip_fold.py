"""Per-edge half-hip via the partial skeleton fold (general shapes).

Golden anchor: a rectangle with BOTH cap edges clipped at the same depth via
the fold must match the exact calculator's HALF_HIP result. Then the fold is
exercised where the exact calculator cannot go: a single end, and L-shapes.
"""
import math

import pytest

from tectora.core.footprint import Footprint
from tectora.core.gable import GableNotSupported
from tectora.core.roof_generator import generate_roof

RECT = [(0, 0), (10, 0), (10, 6), (0, 6)]
L_SHAPE = [(0, 0), (12, 0), (12, 4), (4, 4), (4, 10), (0, 10)]
PITCH = 35.0
D = 1.2          # clip depth in metres (cap width 6 → natural apex at 3)


def _rect_with_depths(depths):
    return Footprint.from_lists(RECT, ["eave"] * 4, depths)


def _cap_edge_indices(fp):
    """Indices of the two short (cap) edges of the normalized rectangle."""
    lens = [(fp.edge_length(i), i) for i in range(fp.n)]
    lens.sort()
    return [i for (_l, i) in lens[:2]]


def test_both_ends_fold_matches_exact_calculator():
    fp_plain = Footprint.from_lists(RECT, ["eave"] * 4)
    caps = _cap_edge_indices(fp_plain)
    depths = [0.0] * 4
    for i in caps:
        depths[i] = D
    folded = generate_roof(Footprint.from_lists(RECT, ["eave"] * 4, depths), PITCH)

    # Exact path: gabled rectangle + symmetric clip depth.
    fp_gable = Footprint.from_lists(
        RECT, ["gable" if i in caps else "eave" for i in range(4)])
    exact = generate_roof(fp_gable, PITCH, half_hip_m=D)

    assert folded.total_area_m2 == pytest.approx(exact.total_area_m2, rel=1e-6)
    assert folded.ridge_length_m == pytest.approx(exact.ridge_length_m, abs=1e-6)
    assert folded.hip_length_m == pytest.approx(exact.hip_length_m, rel=1e-6)
    assert folded.eaves_perimeter_m == pytest.approx(exact.eaves_perimeter_m)
    assert folded.apex_height_m == pytest.approx(exact.apex_height_m)


def test_single_end_half_hip_on_rectangle():
    fp_plain = Footprint.from_lists(RECT, ["eave"] * 4)
    caps = _cap_edge_indices(fp_plain)
    depths = [0.0] * 4
    depths[caps[0]] = D
    result = generate_roof(_rect_with_depths(depths), PITCH)
    plain = generate_roof(fp_plain, PITCH)

    # Ridge grows by exactly (W/2 − d) on the clipped end only.
    assert result.ridge_length_m == pytest.approx(
        plain.ridge_length_m + (3.0 - D), abs=1e-6)
    # Still 4 hip rafters; the clipped pair is shorter.
    assert result.hip_count == 4
    assert result.hip_length_m < plain.hip_length_m
    # The end face is steeper than the main pitch, so total area grows… no —
    # the steeper face covers the SAME plan area with more surface, but the
    # plan split is different: total must simply exceed the plain hip's.
    assert result.total_area_m2 > plain.total_area_m2
    # Eaves unchanged: the clipped cap edge is still an eave.
    assert result.eaves_perimeter_m == pytest.approx(plain.eaves_perimeter_m)


def test_end_face_gets_the_requested_pitch():
    from tectora.core.roof_generator import half_hip_depth_for_pitch
    target = 60.0
    d = half_hip_depth_for_pitch(6.0, PITCH, target)
    fp_plain = Footprint.from_lists(RECT, ["eave"] * 4)
    caps = _cap_edge_indices(fp_plain)
    depths = [0.0] * 4
    depths[caps[0]] = d
    result = generate_roof(_rect_with_depths(depths), PITCH)
    slopes = sorted(round(f.slope_deg, 2) for f in result.faces)
    assert slopes.count(round(PITCH, 2)) == 3        # two mains + one plain hip
    assert round(target, 2) in slopes                # the clipped end


def test_half_hip_on_l_shape_wing_end():
    fp = Footprint.from_lists(L_SHAPE, ["eave"] * 6)
    # Find a valid wing-end cap (the L has two).
    from tectora.core.gable import is_valid_gable_edge
    caps = [i for i in range(fp.n) if is_valid_gable_edge(fp, i)]
    assert caps
    depths = [0.0] * 6
    depths[caps[0]] = 1.0
    clipped = generate_roof(Footprint.from_lists(L_SHAPE, ["eave"] * 6, depths), PITCH)
    plain = generate_roof(fp, PITCH)

    cap_w = fp.edge_length(caps[0])
    assert clipped.ridge_length_m == pytest.approx(
        plain.ridge_length_m + (cap_w / 2.0 - 1.0), abs=1e-6)
    assert clipped.valley_length_m == pytest.approx(plain.valley_length_m)
    assert clipped.total_area_m2 > plain.total_area_m2
    # The clipped end face reports its own steeper pitch.
    h = (cap_w / 2.0) * math.tan(math.radians(PITCH))
    beta = math.degrees(math.atan(h / 1.0))
    assert any(abs(f.slope_deg - beta) < 0.01 for f in clipped.faces)


def test_depth_beyond_natural_apex_keeps_plain_hip():
    fp_plain = Footprint.from_lists(RECT, ["eave"] * 4)
    caps = _cap_edge_indices(fp_plain)
    depths = [0.0] * 4
    depths[caps[0]] = 3.0            # exactly W/2 — nothing to clip
    result = generate_roof(_rect_with_depths(depths), PITCH)
    plain = generate_roof(fp_plain, PITCH)
    assert result.ridge_length_m == pytest.approx(plain.ridge_length_m)
    assert result.total_area_m2 == pytest.approx(plain.total_area_m2)


def test_half_hip_on_invalid_edge_is_rejected():
    depths = [0.0] * 6
    # Edge 0 of the L (the long bottom edge) is not a wing-end cap.
    fp = Footprint.from_lists(L_SHAPE, ["eave"] * 6, [2.0, 0, 0, 0, 0, 0])
    from tectora.core.gable import is_valid_gable_edge
    # Locate the entry that landed on an invalid edge after normalization.
    bad = [i for i in range(fp.n)
           if fp.edges[i].half_hip_m > 0 and not is_valid_gable_edge(fp, i)]
    if not bad:                       # normalization put it on a valid cap
        pytest.skip("depth landed on a valid cap after normalization")
    with pytest.raises(GableNotSupported):
        generate_roof(fp, PITCH)


def test_edge_half_hip_serialization_roundtrip():
    from tectora.application.project import Building
    fp = Footprint.from_lists(RECT, ["eave"] * 4, [0.0, 1.5, 0.0, 0.0])
    b = Building(footprint=fp, pitch_deg=PITCH)
    b2 = Building.from_dict(b.to_dict())
    assert [e.half_hip_m for e in b2.footprint.edges] == \
           [e.half_hip_m for e in fp.edges]


def test_footprint_normalization_keeps_depth_on_its_edge():
    # Drawn CW: from_lists reverses to CCW and rotates the edge list — the
    # depth must stay attached to the same geometric edge.
    cw = list(reversed(RECT))
    depths = [0.0] * 4
    types = ["eave"] * 4
    depths[1] = 2.0                   # some edge of the CW drawing
    fp = Footprint.from_lists(cw, types, depths)
    carrying = [i for i in range(fp.n) if fp.edges[i].half_hip_m > 0]
    assert len(carrying) == 1
    i = carrying[0]
    a, bpt = fp.vertices[i], fp.vertices[(i + 1) % fp.n]
    # The CW drawing's edge 1 runs between these two points.
    expect = {tuple(cw[1]), tuple(cw[2])}
    assert {tuple(map(float, a)), tuple(map(float, bpt))} == \
           {tuple(map(float, p)) for p in expect}
