"""Dispatch: rectangle → exact calculator; general polygon → straight skeleton.

Tectora models the ROOF, not the house footprint: the drawn polygon is the
**eave (roof) outline**, so the roof geometry is built on it directly. The eave
overhang only insets the *walls* inward (handled by the 3D view) — it does NOT
change the roof area/eaves and does NOT suppress gables.
"""
from __future__ import annotations

import dataclasses

from tectora.core.bridge import UnsupportedFootprint, footprint_to_spec
from tectora.core.calculator import calculate
from tectora.core.footprint import Footprint, drop_collinear_vertices
from tectora.core.result import RoofResult
from tectora.core.roof_from_skeleton import build_roof
from tectora.core.spec import RoofType


def generate_roof(
    footprint: Footprint,
    pitch_deg: float,
    name: str = "",
    overhang: float = 0.0,
    half_hip_m: float = 0.0,
) -> RoofResult:
    # Drop accidental mid-edge (collinear) points first: they don't change the
    # outline but would break the skeleton's corner-count self-check. This also
    # lets a split rectangle be recognised as a rectangle again.
    sv, se = drop_collinear_vertices(
        list(footprint.vertices), [e.type.value for e in footprint.edges]
    )
    if len(sv) != footprint.n:
        footprint = Footprint.from_lists(sv, se)

    # The drawn polygon IS the roof eave outline. ``overhang`` does not affect the
    # roof geometry (it only insets the walls in the 3D view), so it is ignored
    # here — gables are therefore honoured at any overhang.
    try:
        spec = footprint_to_spec(footprint, pitch_deg, name=name)
    except UnsupportedFootprint:
        return build_roof(footprint, pitch_deg)   # general polygon → SS

    # Half-hip (polovalba): a GABLE rectangle clipped by depth *half_hip_m*
    # becomes a half-hip. d = half_hip_m along the ridge; ratio = 2·d/W
    # (0 → gable, 1 → full hip). Falls back to the plain gable if the resulting
    # spec is degenerate (e.g. a square clipped to a zero-length ridge).
    if half_hip_m > 0 and spec.type == RoofType.GABLE and spec.width > 0:
        ratio = min(1.0, 2.0 * half_hip_m / spec.width)
        if ratio > 0:
            try:
                hh = dataclasses.replace(
                    spec, type=RoofType.HALF_HIP, half_hip_ratio=ratio)
                return calculate(hh)
            except Exception:
                pass  # degenerate → keep the plain gable below

    return calculate(spec)                         # rectangle → exact
