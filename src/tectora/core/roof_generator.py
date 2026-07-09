"""Dispatch: rectangle → exact calculator; general polygon → straight skeleton.

Tectora models the ROOF, not the house footprint: the drawn polygon is the
**eave (roof) outline**, so the roof geometry is built on it directly. The eave
overhang only insets the *walls* inward (handled by the 3D view) — it does NOT
change the roof area/eaves and does NOT suppress gables.

Every RoofResult leaving :func:`generate_roof` is in MODEL coordinates: the
exact calculator's faces are mapped through the bridge's CalcFrame and its
interior edges are emitted as ``skeleton_segments``, exactly like the straight-
skeleton path. Consumers therefore never re-derive the placement themselves —
the historic source of 2D-vs-3D orientation mismatches.
"""
from __future__ import annotations

import dataclasses
import math

from tectora.core.bridge import (
    CalcFrame,
    UnsupportedFootprint,
    footprint_to_spec_frame,
)
from tectora.core.calculator import calculate
from tectora.core.footprint import Footprint, drop_collinear_vertices
from tectora.core.result import FaceResult, RoofResult
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
        spec, frame = footprint_to_spec_frame(footprint, pitch_deg, name=name)
    except UnsupportedFootprint:
        # General polygon, or a rectangle combination the exact calculator has
        # no type for (e.g. a single gabled short end = 3 eaves) — the
        # skeleton fold handles the valid ones and raises on the rest.
        return build_roof(footprint, pitch_deg)

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
                return _to_model_frame(calculate(hh), frame)
            except Exception:
                pass  # degenerate → keep the plain gable below

    return _to_model_frame(calculate(spec), frame)   # rectangle → exact


def _interior_segments(faces: list) -> list:
    """Interior (shared) edges of a face set, classified ridge / hip.

    Mirrors the straight-skeleton ``skeleton_segments`` format:
    ``(kind, (x, y, z), (x, y, z))`` — an edge used by ≥ 2 faces is a ridge
    when both its endpoints are level above the eave plane, otherwise a hip.
    """
    edge_data: dict = {}
    for face in faces:
        vs = face.vertices
        m = len(vs)
        for i in range(m):
            a, b = vs[i], vs[(i + 1) % m]
            ka = tuple(round(c, 6) for c in a)
            kb = tuple(round(c, 6) for c in b)
            if ka == kb:
                continue
            key = frozenset((ka, kb))
            item = edge_data.setdefault(key, [a, b, 0])
            item[2] += 1

    tol = 1e-6
    segments: list = []
    for a, b, count in edge_data.values():
        if count < 2:
            continue
        if a[2] > tol and b[2] > tol and abs(a[2] - b[2]) < tol:
            kind = "ridge"
        else:
            kind = "hip"
        segments.append((kind, tuple(a), tuple(b)))
    return segments


def _to_model_frame(result: RoofResult, frame: CalcFrame) -> RoofResult:
    """Map an exact-calculator result into model coordinates.

    Transforms every face through *frame* and derives ``skeleton_segments``
    from the shared face edges, so the exact path emits the same model-space
    geometry contract as the straight-skeleton path. Measurements (areas,
    lengths) are placement-independent and stay untouched.
    """
    faces = [
        FaceResult(
            name=f.name,
            area_m2=f.area_m2,
            slope_deg=f.slope_deg,
            vertices=[(*frame.to_model(v[0], v[1]), v[2]) for v in f.vertices],
        )
        for f in result.faces
    ]
    return dataclasses.replace(
        result, faces=faces, skeleton_segments=_interior_segments(faces))
