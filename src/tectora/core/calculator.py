"""Roof calculation core.

Coordinate system:
    x — along `length` (longer dimension)
    y — along `width`
    z — upward
    Footprint corner at (0, 0, 0), opposite corner at (L, W, 0).

All formulae are derived from standard geometry and are covered by tests
(tests/test_calculator.py). For each type we maintain the invariant
"sum of face areas × cos(α) = footprint area" where applicable.
"""
from __future__ import annotations

import math

from tectora.core.result import FaceResult, RoofResult
from tectora.core.spec import RoofSpec, RoofType


def calculate(spec: RoofSpec) -> RoofResult:
    """Main entry point. Computes the complete output for the given spec."""
    spec.validate()

    dispatch = {
        RoofType.SHED: _calc_shed,
        RoofType.GABLE: _calc_gable,
        RoofType.HIP: _calc_hip,
        RoofType.HALF_HIP: _calc_half_hip,
        RoofType.PYRAMID: _calc_pyramid,
    }
    return dispatch[spec.type](spec)


def _calc_shed(spec: RoofSpec) -> RoofResult:
    """Shed (mono-pitch): single sloped plane, higher edge at y = W."""
    L = spec.roof_length
    W = spec.roof_width
    alpha = math.radians(spec.pitch)

    apex_h = W * math.tan(alpha)
    slope_len = W / math.cos(alpha)

    area = L * slope_len
    face = FaceResult(
        name="pultová rovina",
        area_m2=area,
        slope_deg=spec.pitch,
        vertices=[(0, 0, 0), (L, 0, 0), (L, W, apex_h), (0, W, apex_h)],
    )

    # Gable ends: left (x=0) and right (x=L) — triangles with a sloped edge
    # of length slope_len. Edge tile = full sloped edge.
    gable_edge = slope_len

    return RoofResult(
        spec=spec,
        total_area_m2=area,
        ridge_length_m=0.0,
        hip_length_m=0.0,
        hip_count=0,
        valley_length_m=0.0,
        gable_edge_left_m=gable_edge,
        gable_edge_right_m=gable_edge,
        # Eaves run along y=0 (front) and y=W (rear, higher).
        # In practice the higher side gets a gable finish, but for length
        # calculations both long edges are counted.
        eaves_perimeter_m=2 * L,
        apex_height_m=apex_h,
        faces=[face],
    )


def _calc_gable(spec: RoofSpec) -> RoofResult:
    """Gable: two sloped planes, ridge along x.

    Symmetric when pitch_back is None. For an asymmetric roof the apex
    is derived from the equal-height condition of both planes.
    """
    L = spec.roof_length
    W = spec.roof_width
    alpha_f = math.radians(spec.pitch)
    alpha_b = math.radians(spec.pitch_back if spec.pitch_back is not None else spec.pitch)

    # Apex height and ridge position along y-axis
    # Front face base at y=0, back face base at y=W.
    # Apex at y=y_apex, height h.
    # h = y_apex * tan(α_f) = (W - y_apex) * tan(α_b)
    y_apex = W * math.tan(alpha_b) / (math.tan(alpha_f) + math.tan(alpha_b))
    apex_h = y_apex * math.tan(alpha_f)

    slope_len_f = y_apex / math.cos(alpha_f)
    slope_len_b = (W - y_apex) / math.cos(alpha_b)

    area_f = L * slope_len_f
    area_b = L * slope_len_b
    total_area = area_f + area_b

    face_f = FaceResult(
        name="přední rovina (sedlová)",
        area_m2=area_f,
        slope_deg=spec.pitch,
        vertices=[(0, 0, 0), (L, 0, 0), (L, y_apex, apex_h), (0, y_apex, apex_h)],
    )
    face_b = FaceResult(
        name="zadní rovina (sedlová)",
        area_m2=area_b,
        slope_deg=math.degrees(alpha_b),
        vertices=[(0, W, 0), (L, W, 0), (L, y_apex, apex_h), (0, y_apex, apex_h)],
    )

    gable_edge = slope_len_f + slope_len_b  # two sloped edges per gable end, summed

    return RoofResult(
        spec=spec,
        total_area_m2=total_area,
        ridge_length_m=L,
        hip_length_m=0.0,
        hip_count=0,
        valley_length_m=0.0,
        gable_edge_left_m=gable_edge,
        gable_edge_right_m=gable_edge,
        eaves_perimeter_m=2 * L,
        apex_height_m=apex_h,
        faces=[face_f, face_b],
    )


def _calc_hip(spec: RoofSpec) -> RoofResult:
    """Hip: 4 slopes meeting at the ridge, all at the same pitch."""
    L = spec.roof_length
    W = spec.roof_width
    alpha = math.radians(spec.pitch)

    if L < W:
        # Geometry requires L >= W; otherwise the ridge would be "perpendicular" or zero.
        # If L == W → pyramid; if L < W → flipped orientation.
        # For simplicity, swap the orientation.
        L, W = W, L

    apex_h = (W / 2) * math.tan(alpha)
    slope_len = (W / 2) / math.cos(alpha)
    ridge_len = L - W

    # 2 trapezoidal planes (front and back): base L, top edge L-W
    trapezoid_area = (L + (L - W)) / 2 * slope_len  # = (2L - W) * slope_len / 2
    # 2 triangular planes (hip ends) at the short ends: base W, height slope_len
    triangle_area = 0.5 * W * slope_len
    total_area = 2 * trapezoid_area + 2 * triangle_area
    # Sanity check: total_area == L * W / cos(α)
    # = 2 * (2L-W)/2 * (W/2)/cos + 2 * W²/(4 cos) = W(2L-W)/(2cos) + W²/(2cos)
    # = W(2L - W + W) / (2 cos) = L*W / cos ✓

    ridge_z = apex_h
    # Ridge endpoints in plan (W/2, W/2) and (L - W/2, W/2)
    rx0, rx1 = W / 2, L - W / 2

    face_front = FaceResult(
        name="přední rovina (valbová)",
        area_m2=trapezoid_area,
        slope_deg=spec.pitch,
        vertices=[(0, 0, 0), (L, 0, 0), (rx1, W / 2, ridge_z), (rx0, W / 2, ridge_z)],
    )
    face_back = FaceResult(
        name="zadní rovina (valbová)",
        area_m2=trapezoid_area,
        slope_deg=spec.pitch,
        vertices=[(0, W, 0), (L, W, 0), (rx1, W / 2, ridge_z), (rx0, W / 2, ridge_z)],
    )
    face_left = FaceResult(
        name="levá valba",
        area_m2=triangle_area,
        slope_deg=spec.pitch,
        vertices=[(0, 0, 0), (0, W, 0), (rx0, W / 2, ridge_z)],
    )
    face_right = FaceResult(
        name="pravá valba",
        area_m2=triangle_area,
        slope_deg=spec.pitch,
        vertices=[(L, 0, 0), (L, W, 0), (rx1, W / 2, ridge_z)],
    )

    # 4 hip rafters: each from a corner (e.g. 0,0,0) to a ridge end (W/2, W/2, h)
    hip_len = math.sqrt((W / 2) ** 2 + (W / 2) ** 2 + apex_h ** 2)

    return RoofResult(
        spec=spec,
        total_area_m2=total_area,
        ridge_length_m=ridge_len,
        hip_length_m=4 * hip_len,
        hip_count=4,
        valley_length_m=0.0,
        gable_edge_left_m=0.0,
        gable_edge_right_m=0.0,
        eaves_perimeter_m=2 * L + 2 * W,
        apex_height_m=apex_h,
        faces=[face_front, face_back, face_left, face_right],
    )


def _calc_half_hip(spec: RoofSpec) -> RoofResult:
    """Half-hip (clipped gable): the end hip is a steeper triangle
    running from the footprint corners up to the ridge end.

    Boundary cases:
        ratio = 0  → gable (no hip)
        ratio = 1  → full hip (hip slope == main pitch)
    """
    if spec.half_hip_ratio <= 0:
        # delegate to gable
        return _calc_gable(spec)
    if spec.half_hip_ratio >= 1:
        return _calc_hip(spec)

    L = spec.roof_length
    W = spec.roof_width
    alpha = math.radians(spec.pitch)

    d = spec.half_hip_ratio * (W / 2)         # hip overhang depth
    apex_h = (W / 2) * math.tan(alpha)
    slope_len = (W / 2) / math.cos(alpha)
    ridge_len = L - 2 * d

    # Main slope: trapezoid with bottom edge L, top edge L - 2d, height slope_len
    trapezoid_area = (L + ridge_len) / 2 * slope_len
    # Hip end: triangle (0,0,0), (0,W,0), (d, W/2, apex_h)
    # The face is a plane inclined at β = atan(h/d).
    # Area = (1/2) * W * slant, where slant is the triangle height in its own plane
    # = √(d² + h²) (from midpoint of the base to the apex)
    end_slant = math.sqrt(d ** 2 + apex_h ** 2)
    triangle_area = 0.5 * W * end_slant
    beta_deg = math.degrees(math.atan(apex_h / d))

    total_area = 2 * trapezoid_area + 2 * triangle_area

    rx0, rx1 = d, L - d
    face_front = FaceResult(
        name="přední rovina",
        area_m2=trapezoid_area,
        slope_deg=spec.pitch,
        vertices=[(0, 0, 0), (L, 0, 0), (rx1, W / 2, apex_h), (rx0, W / 2, apex_h)],
    )
    face_back = FaceResult(
        name="zadní rovina",
        area_m2=trapezoid_area,
        slope_deg=spec.pitch,
        vertices=[(0, W, 0), (L, W, 0), (rx1, W / 2, apex_h), (rx0, W / 2, apex_h)],
    )
    face_left = FaceResult(
        name="levá valbička",
        area_m2=triangle_area,
        slope_deg=beta_deg,
        vertices=[(0, 0, 0), (0, W, 0), (rx0, W / 2, apex_h)],
    )
    face_right = FaceResult(
        name="pravá valbička",
        area_m2=triangle_area,
        slope_deg=beta_deg,
        vertices=[(L, 0, 0), (L, W, 0), (rx1, W / 2, apex_h)],
    )

    # 4 hip rafters: corner (0,0,0) → ridge end (d, W/2, h)
    hip_len = math.sqrt(d ** 2 + (W / 2) ** 2 + apex_h ** 2)

    return RoofResult(
        spec=spec,
        total_area_m2=total_area,
        ridge_length_m=ridge_len,
        hip_length_m=4 * hip_len,
        hip_count=4,
        valley_length_m=0.0,
        gable_edge_left_m=0.0,
        gable_edge_right_m=0.0,
        eaves_perimeter_m=2 * L + 2 * W,
        apex_height_m=apex_h,
        faces=[face_front, face_back, face_left, face_right],
        notes=[
            f"Sklon valby vyšel β = {beta_deg:.1f}°, "
            f"strmější než hlavní sklon α = {spec.pitch:.1f}°."
        ],
    )


def _calc_pyramid(spec: RoofSpec) -> RoofResult:
    """Pyramid: 4 triangles meeting at the apex, square footprint."""
    L = spec.roof_length  # = roof_width
    W = spec.roof_width
    alpha = math.radians(spec.pitch)

    apex_h = (L / 2) * math.tan(alpha)
    slope_len = (L / 2) / math.cos(alpha)
    triangle_area = 0.5 * L * slope_len
    total_area = 4 * triangle_area

    apex = (L / 2, L / 2, apex_h)
    face_s = FaceResult("jižní (y=0)", triangle_area, spec.pitch, [(0, 0, 0), (L, 0, 0), apex])
    face_n = FaceResult("severní (y=W)", triangle_area, spec.pitch, [(0, W, 0), (L, W, 0), apex])
    face_w = FaceResult("západní (x=0)", triangle_area, spec.pitch, [(0, 0, 0), (0, W, 0), apex])
    face_e = FaceResult("východní (x=L)", triangle_area, spec.pitch, [(L, 0, 0), (L, W, 0), apex])

    hip_len = math.sqrt((L / 2) ** 2 + (L / 2) ** 2 + apex_h ** 2)

    return RoofResult(
        spec=spec,
        total_area_m2=total_area,
        ridge_length_m=0.0,
        hip_length_m=4 * hip_len,
        hip_count=4,
        valley_length_m=0.0,
        gable_edge_left_m=0.0,
        gable_edge_right_m=0.0,
        eaves_perimeter_m=4 * L,
        apex_height_m=apex_h,
        faces=[face_s, face_n, face_w, face_e],
    )
