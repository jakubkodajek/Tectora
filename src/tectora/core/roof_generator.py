"""Dispatch: rectangle → exact calculator; general polygon → straight skeleton.

Tectora models the ROOF, not the house footprint: the drawn polygon is the
**eave (roof) outline**, so the roof geometry is built on it directly. The eave
overhang only insets the *walls* inward (handled by the 3D view) — it does NOT
change the roof area/eaves and does NOT suppress gables.
"""
from __future__ import annotations

import dataclasses
import math

from tectora.core.bridge import UnsupportedFootprint, footprint_to_spec
from tectora.core.calculator import calculate
from tectora.core.footprint import Footprint, drop_collinear_vertices
from tectora.core.result import RoofResult
from tectora.core.roof_from_skeleton import build_roof
from tectora.core.spec import RoofType


def half_hip_depth_for_pitch(
    width: float,
    pitch_deg: float,
    end_pitch_deg: float,
) -> float:
    """Half-hip clip depth *d* that gives the end face the pitch *end_pitch_deg*.

    The ridge keeps the main slopes' height H = (W/2)·tan(pitch); the end face
    rises H over the horizontal run d, so β = atan(H/d) ⇒ d = H / tan(β).
    A steeper end (β → 90°) means a shallower clip (d → 0, almost a gable);
    β equal to the main pitch means d = W/2 (a full hip). *end_pitch_deg*
    must therefore be greater than *pitch_deg*.
    """
    if not (0 < pitch_deg < 90 and 0 < end_pitch_deg < 90):
        raise ValueError("Sklony musí být v rozsahu 1–89°.")
    if end_pitch_deg <= pitch_deg:
        raise ValueError(
            "Sklon polovalby musí být větší než hlavní sklon střechy.")
    h = (width / 2.0) * math.tan(math.radians(pitch_deg))
    return h / math.tan(math.radians(end_pitch_deg))


def generate_roof(
    footprint: Footprint,
    pitch_deg: float,
    name: str = "",
    overhang: float = 0.0,
    half_hip_m: float = 0.0,
    pitch_back: float | None = None,
) -> RoofResult:
    # Drop accidental mid-edge (collinear) points first: they don't change the
    # outline but would break the skeleton's corner-count self-check. This also
    # lets a split rectangle be recognised as a rectangle again.
    sv, se, sh = drop_collinear_vertices(
        list(footprint.vertices),
        [e.type.value for e in footprint.edges],
        half_hips=[e.half_hip_m for e in footprint.edges],
    )
    if len(sv) != footprint.n:
        footprint = Footprint.from_lists(sv, se, sh)

    # Per-edge half-hips (clicked end triangles) need the skeleton fold — the
    # exact calculator only knows the symmetric building-level clip. This also
    # covers a single-ended polovalba on a plain rectangle.
    if any(e.half_hip_m > 0 for e in footprint.edges):
        return build_roof(footprint, pitch_deg)

    # The drawn polygon IS the roof eave outline. ``overhang`` does not affect the
    # roof geometry (it only insets the walls in the 3D view), so it is ignored
    # here — gables are therefore honoured at any overhang.
    try:
        spec = footprint_to_spec(footprint, pitch_deg, name=name)
    except UnsupportedFootprint:
        return build_roof(footprint, pitch_deg)   # general polygon → SS

    # Asymmetric gable: a different back-slope pitch on a GABLE rectangle (the
    # two long slopes get pitch_deg / pitch_back instead of a single pitch).
    if (pitch_back is not None and spec.type == RoofType.GABLE
            and 0 < pitch_back < 90 and abs(pitch_back - pitch_deg) > 1e-9):
        try:
            spec = dataclasses.replace(spec, pitch_back=float(pitch_back))
        except Exception:
            pass

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
