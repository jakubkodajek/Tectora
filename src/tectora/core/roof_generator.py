"""Dispatch: rectangle → exact calculator; general polygon → straight skeleton."""
from __future__ import annotations

from tectora.core.bridge import UnsupportedFootprint, footprint_to_spec
from tectora.core.calculator import calculate
from tectora.core.footprint import Footprint
from tectora.core.geom2d import EPS
from tectora.core.poly_offset import offset_polygon
from tectora.core.result import RoofResult
from tectora.core.roof_from_skeleton import build_roof


def generate_roof(
    footprint: Footprint,
    pitch_deg: float,
    name: str = "",
    overhang: float = 0.0,
) -> RoofResult:
    # If overhang is requested, offset the footprint outward and always use the
    # straight-skeleton path so faces are in real model coordinates (matching the
    # offset eave outline drawn on the 2D canvas).
    if overhang > EPS:
        offset_verts = offset_polygon(list(footprint.vertices), overhang)
        n = len(offset_verts)
        offset_fp = Footprint.from_lists(offset_verts, ["eave"] * n)
        return build_roof(offset_fp, pitch_deg)

    try:
        spec = footprint_to_spec(footprint, pitch_deg, name=name)
    except UnsupportedFootprint:
        return build_roof(footprint, pitch_deg)   # general polygon → SS
    return calculate(spec)                         # rectangle → exact
