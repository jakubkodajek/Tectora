# -*- coding: utf-8 -*-
"""Pure tests for dimension annotation helpers in skeleton_render (G4b).

No Qt required — all pure Python / calculator calls.
"""
import math
from tectora.core.calculator import calculate
from tectora.core.spec import RoofSpec, RoofType
from tectora.gui.editor.skeleton_render import (classify_skeleton, face_area_labels,
                                                fmt_area, fmt_len)


def test_fmt_czech():
    assert fmt_len(4.0) == "4,00 m"
    assert fmt_area(4.62) == "4,62 m²"


def test_hip_segment_length_matches_total():
    r = calculate(RoofSpec(RoofType.HIP, 12, 8, 35))
    segs = classify_skeleton(r)
    hips = [s for s in segs if s[0] == "hip"]
    assert len(hips) == 4
    per = r.hip_length_m / r.hip_count
    assert all(abs(s[3] - per) < 1e-6 for s in hips)


def test_ridge_segment_length_matches():
    r = calculate(RoofSpec(RoofType.HIP, 12, 8, 35))
    ridge = [s for s in classify_skeleton(r) if s[0] == "ridge"]
    assert len(ridge) == 1
    assert abs(ridge[0][3] - r.ridge_length_m) < 1e-6


def test_face_area_labels_count_and_values():
    r = calculate(RoofSpec(RoofType.HIP, 12, 8, 35))
    labels = face_area_labels(r)
    assert len(labels) == len(r.faces)
    assert all(a > 0 for (_x, _y, a) in labels)
