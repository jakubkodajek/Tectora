"""Dispatch: rectangle → exact calculator; general polygon → straight skeleton."""
from __future__ import annotations

from tectora.core.bridge import UnsupportedFootprint, footprint_to_spec
from tectora.core.calculator import calculate
from tectora.core.footprint import Footprint
from tectora.core.result import RoofResult
from tectora.core.roof_from_skeleton import build_roof


def generate_roof(footprint: Footprint, pitch_deg: float, name: str = "") -> RoofResult:
    try:
        spec = footprint_to_spec(footprint, pitch_deg, name=name)
    except UnsupportedFootprint:
        return build_roof(footprint, pitch_deg)   # general polygon → SS
    return calculate(spec)                         # rectangle → exact
