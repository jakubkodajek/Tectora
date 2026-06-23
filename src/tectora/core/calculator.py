"""Výpočetní jádro střech.

Souřadný systém:
    x — podél `length` (delší rozměr)
    y — podél `width`
    z — vzhůru
    Roh půdorysu u (0, 0, 0), opačný u (L, W, 0).

Všechny vzorce čerpají z platné geometrie a jsou pokryty testy
(tests/test_calculator.py). U každého typu zachováváme invariant
"součet ploch rovin × cos(α) = plocha půdorysu" pokud to typ splňuje.
"""
from __future__ import annotations

import math

from tectora.core.result import FaceResult, RoofResult
from tectora.core.spec import RoofSpec, RoofType


def calculate(spec: RoofSpec) -> RoofResult:
    """Hlavní vstupní bod. Spočítá kompletní výstup pro daný spec."""
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
    """Pultová: jediná sklonová rovina, vyšší okraj na straně y = W."""
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

    # Štíty: levý (x=0) a pravý (x=L) — trojúhelníky se šikmou hranou
    # délky slope_len. Krajová taška = celá šikmá hrana.
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
        # Okap běží podél y=0 (přední) a y=W (zadní vyšší).
        # V praxi se na vyšší straně dělá štítová úprava, ale pro výpočet
        # délek se počítají obě dlouhé hrany.
        eaves_perimeter_m=2 * L,
        apex_height_m=apex_h,
        faces=[face],
    )


def _calc_gable(spec: RoofSpec) -> RoofResult:
    """Sedlová: dvě sklonové roviny, hřeben podél x.

    Symetrická pokud pitch_back is None. Při asymetrické vychází apex
    z podmínky stejné výšky obou rovin (skutečné situace).
    """
    L = spec.roof_length
    W = spec.roof_width
    alpha_f = math.radians(spec.pitch)
    alpha_b = math.radians(spec.pitch_back if spec.pitch_back is not None else spec.pitch)

    # Apex height a poloha hřebene v ose y
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

    gable_edge = slope_len_f + slope_len_b  # dvě hrany na štítu, sečteno

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
    """Valbová: 4 sklony do hřebene, všechny stejně skloněné."""
    L = spec.roof_length
    W = spec.roof_width
    alpha = math.radians(spec.pitch)

    if L < W:
        # Geometrie vyžaduje L >= W, jinak je hřeben "kolmo" nebo nulový.
        # Pokud L == W → pyramida; pokud L < W → otočená situace.
        # Pro jednoduchost přehodíme orientaci.
        L, W = W, L

    apex_h = (W / 2) * math.tan(alpha)
    slope_len = (W / 2) / math.cos(alpha)
    ridge_len = L - W

    # 2 trapezoidové roviny (přední a zadní): základna L, vrchní hrana L-W
    trapezoid_area = (L + (L - W)) / 2 * slope_len  # = (2L - W) * slope_len / 2
    # 2 trojúhelníkové roviny (valby) na bočních koncích: základna W, výška slope_len
    triangle_area = 0.5 * W * slope_len
    total_area = 2 * trapezoid_area + 2 * triangle_area
    # Sanity check: total_area == L * W / cos(α)
    # = 2 * (2L-W)/2 * (W/2)/cos + 2 * W²/(4 cos) = W(2L-W)/(2cos) + W²/(2cos)
    # = W(2L - W + W) / (2 cos) = L*W / cos ✓

    ridge_z = apex_h
    # Ridge endpoints in plan (W/2, W/2) a (L - W/2, W/2)
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

    # 4 nároží: každé z rohu (např. 0,0,0) do konce hřebene (W/2, W/2, h)
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
    """Polovalbová (clipped gable): krajní valba je strmější trojúhelník
    od rohů půdorysu až ke konci hřebene.

    Mezní případy:
        ratio = 0  → sedlová (žádná valba)
        ratio = 1  → valbová (plná valba se sklonem = hlavní sklon)
    """
    if spec.half_hip_ratio <= 0:
        # delegate to gable
        return _calc_gable(spec)
    if spec.half_hip_ratio >= 1:
        return _calc_hip(spec)

    L = spec.roof_length
    W = spec.roof_width
    alpha = math.radians(spec.pitch)

    d = spec.half_hip_ratio * (W / 2)         # přesah valby
    apex_h = (W / 2) * math.tan(alpha)
    slope_len = (W / 2) / math.cos(alpha)
    ridge_len = L - 2 * d

    # Hlavní sklon: lichoběžník dolní hrana L, horní hrana L - 2d, výška slope_len
    trapezoid_area = (L + ridge_len) / 2 * slope_len
    # Valba: trojúhelník (0,0,0), (0,W,0), (d, W/2, apex_h)
    # Strana je rovinou skloněnou pod úhlem β = atan(h/d).
    # Plocha = (1/2) * W * slant, kde slant je výška trojúhelníku v jeho rovině
    # = √(d² + h²) (od středu základny k vrcholu)
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

    # 4 nároží: roh (0,0,0) → konec hřebene (d, W/2, h)
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
    """Stanová: 4 trojúhelníky do vrcholu, čtvercový půdorys."""
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
