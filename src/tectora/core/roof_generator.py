"""Dispatch: rectangle → exact calculator; general polygon → straight skeleton.

Tectora models the ROOF, not the house footprint: the drawn polygon is the
**eave (roof) outline**, so the roof geometry is built on it directly. The eave
overhang only insets the *walls* inward (handled by the 3D view) — it does NOT
change the roof area/eaves and does NOT suppress gables.
"""
from __future__ import annotations

from tectora.core.bridge import UnsupportedFootprint, footprint_to_spec
from tectora.core.calculator import calculate
from tectora.core.footprint import Footprint, drop_collinear_vertices
from tectora.core.result import RoofResult
from tectora.core.roof_from_skeleton import build_roof


def generate_roof(
    footprint: Footprint,
    pitch_deg: float,
    name: str = "",
    overhang: float = 0.0,
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
    return calculate(spec)                         # rectangle → exact
