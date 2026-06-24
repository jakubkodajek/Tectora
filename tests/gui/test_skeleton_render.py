# -*- coding: utf-8 -*-
"""Pure tests for skeleton_render helpers (no Qt needed)."""
import math
from tectora.core.calculator import calculate
from tectora.core.spec import RoofSpec, RoofType
from tectora.gui.editor.skeleton_render import classify_skeleton, map_calc_point


def _kinds(result):
    segs = classify_skeleton(result)
    return [s[0] for s in segs]


def test_gable_has_one_ridge_no_hip():
    k = _kinds(calculate(RoofSpec(RoofType.GABLE, 12, 8, 35)))
    assert k.count("ridge") == 1
    assert k.count("hip") == 0


def test_hip_has_ridge_and_four_hips():
    k = _kinds(calculate(RoofSpec(RoofType.HIP, 12, 8, 35)))
    assert k.count("ridge") == 1
    assert k.count("hip") == 4


def test_pyramid_has_no_ridge_four_hips():
    k = _kinds(calculate(RoofSpec(RoofType.PYRAMID, 8, 8, 35)))
    assert k.count("ridge") == 0
    assert k.count("hip") == 4


def test_map_calc_point_orientation():
    # wide bbox (length along x): identity-ish translate
    assert map_calc_point(3, 2, (0, 0, 12, 8)) == (3, 2)
    # tall bbox (length along y): swap
    assert map_calc_point(3, 2, (0, 0, 8, 12)) == (2, 3)
