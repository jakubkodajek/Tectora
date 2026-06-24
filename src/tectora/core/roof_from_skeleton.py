"""Build a RoofResult from the straight-skeleton of a Footprint.

SS4 — classify skeleton arcs into ridge/hip/valley, compute 3-D lengths,
aggregate totals, and validate against the exact calculator for rectangles.
"""
from __future__ import annotations

import math
from math import radians, tan, cos, sqrt

from tectora.core.footprint import Footprint
from tectora.core.polyskel_adapter import polyskel_skeleton
from tectora.core.result import RoofResult
from tectora.core.spec import RoofSpec, RoofType
from tectora.core.straight_skeleton import build_lav, RoofGenerationError

# Rounding precision used when matching node coordinates
_ROUND = 6


def _round2(pt: tuple) -> tuple:
    return (round(pt[0], _ROUND), round(pt[1], _ROUND))


def _shoelace(pts: list) -> float:
    """Signed shoelace area (positive for CCW)."""
    n = len(pts)
    return 0.5 * abs(sum(
        pts[i][0] * pts[(i + 1) % n][1] - pts[(i + 1) % n][0] * pts[i][1]
        for i in range(n)
    ))


def _len3d(a: tuple, b: tuple) -> float:
    """3-D Euclidean distance between two (x, y, z) points."""
    return sqrt((b[0]-a[0])**2 + (b[1]-a[1])**2 + (b[2]-a[2])**2)


def build_roof(footprint: Footprint, pitch_deg: float) -> RoofResult:
    """Derive a RoofResult from the straight skeleton of *footprint*.

    Parameters
    ----------
    footprint : Footprint
        CCW polygon (Footprint.from_lists).
    pitch_deg : float
        Roof pitch in degrees.

    Returns
    -------
    RoofResult  with ridge/hip/valley lengths, total area, eave perimeter,
                apex height, and skeleton_segments populated.
    """
    poly = list(footprint.vertices)
    graph = polyskel_skeleton(poly)
    slope = tan(radians(pitch_deg))

    # -----------------------------------------------------------------------
    # Build a lookup: rounded (x, y) -> t (offset height) for skeleton nodes.
    # Polygon vertices have t = 0 by definition.
    # -----------------------------------------------------------------------
    node_t: dict[tuple, float] = {}
    for x, y, t in graph.nodes:
        key = _round2((x, y))
        # Keep the first (or max) — duplicate coincident nodes get same t.
        if key not in node_t:
            node_t[key] = t
        else:
            # In degenerate cases (coincident duplicate nodes from SS3) take max.
            node_t[key] = max(node_t[key], t)

    # Polygon vertices are eave-level (t = 0).
    poly_set: set[tuple] = {_round2(p) for p in poly}
    for key in poly_set:
        node_t[key] = 0.0

    def _get_t(pt: tuple) -> float:
        key = _round2(pt)
        return node_t.get(key, 0.0)

    def _lift(pt: tuple) -> tuple:
        t = _get_t(pt)
        return (pt[0], pt[1], t * slope)

    # -----------------------------------------------------------------------
    # Determine which original polygon vertices are reflex.
    # -----------------------------------------------------------------------
    lav = build_lav(poly)
    reflex_pts: set[tuple] = set()
    for v in lav.vertices():
        if v.is_reflex:
            reflex_pts.add(_round2(v.point))

    # -----------------------------------------------------------------------
    # Classify arcs.
    # -----------------------------------------------------------------------
    EPS = 1e-6

    classified: list[tuple] = []   # (kind, a3d, b3d)

    for (ax, ay), (bx, by) in graph.arcs:
        a2 = (ax, ay)
        b2 = (bx, by)

        a3 = _lift(a2)
        b3 = _lift(b2)

        # Skip zero-length arcs (2D or 3D).
        seg_len = _len3d(a3, b3)
        if seg_len < EPS:
            continue

        a_is_poly = _round2(a2) in poly_set
        b_is_poly = _round2(b2) in poly_set

        if a_is_poly or b_is_poly:
            # One endpoint is on the polygon boundary (z = 0).
            # The poly endpoint determines whether this is a hip or valley arc.
            poly_pt = _round2(a2) if a_is_poly else _round2(b2)
            if poly_pt in reflex_pts:
                kind = "valley"
            else:
                kind = "hip"
        else:
            # Both endpoints are interior skeleton nodes — an internal ridge edge
            # of the skeleton (horizontal ridge, or a sloped interior edge on
            # non-axis-aligned shapes). Either way it is not a corner rafter.
            kind = "ridge"

        classified.append((kind, a3, b3))

    # -----------------------------------------------------------------------
    # Aggregate.
    # -----------------------------------------------------------------------
    ridge_len = 0.0
    hip_len = 0.0
    valley_len = 0.0
    hip_corners: set = set()
    valley_corners: set = set()

    # Count hips/valleys as RAFTERS (one per polygon corner), not arc segments:
    # on non-axis-aligned shapes a rafter can be split into several arcs by
    # intermediate skeleton nodes, so we tally the distinct corner endpoints.
    for kind, a3, b3 in classified:
        seg = _len3d(a3, b3)
        if kind == "ridge":
            ridge_len += seg
        elif kind == "hip":
            hip_len += seg
            for p in (_round2((a3[0], a3[1])), _round2((b3[0], b3[1]))):
                if p in poly_set:
                    hip_corners.add(p)
        elif kind == "valley":
            valley_len += seg
            for p in (_round2((a3[0], a3[1])), _round2((b3[0], b3[1]))):
                if p in poly_set:
                    valley_corners.add(p)
    hip_count = len(hip_corners)
    valley_count = len(valley_corners)

    # Structural self-check: every convex corner emits one hip rafter, every
    # reflex corner one valley. If the skeleton violates this it is unreliable
    # for this shape — fail loudly (RoofGenerationError is a ValueError, so the
    # GUI/CLI report it gracefully) rather than drawing a wrong roof.
    n_reflex = len(reflex_pts)
    n_convex = footprint.n - n_reflex
    if hip_count != n_convex or valley_count != n_reflex:
        raise RoofGenerationError(
            "Skeleton failed the structural check for this shape "
            f"(hips {hip_count}/{n_convex}, valleys {valley_count}/{n_reflex})."
        )

    # -----------------------------------------------------------------------
    # Totals that don't depend on arc classification.
    # -----------------------------------------------------------------------
    footprint_area = _shoelace(poly)
    total_area = footprint_area / cos(radians(pitch_deg))

    eaves_perimeter = sum(footprint.edge_length(i) for i in range(footprint.n))

    apex_height = max(
        (t * slope for _, _, t in graph.nodes),
        default=0.0,
    )

    # -----------------------------------------------------------------------
    # Build a minimal RoofSpec so RoofResult.as_dict() doesn't crash.
    # We use bounding-box L×W and the given pitch.
    # -----------------------------------------------------------------------
    xs = [p[0] for p in poly]
    ys = [p[1] for p in poly]
    bb_l = max(xs) - min(xs)
    bb_w = max(ys) - min(ys)
    # Ensure L >= W (RoofSpec has no such hard constraint but HIP does internally).
    spec_l, spec_w = (bb_l, bb_w) if bb_l >= bb_w else (bb_w, bb_l)
    _spec = RoofSpec(
        type=RoofType.HIP,
        length=spec_l,
        width=spec_w,
        pitch=pitch_deg,
        name="skeleton",
    )

    return RoofResult(
        spec=_spec,
        total_area_m2=total_area,
        ridge_length_m=ridge_len,
        hip_length_m=hip_len,
        hip_count=hip_count,
        valley_length_m=valley_len,
        valley_count=valley_count,
        gable_edge_left_m=0.0,
        gable_edge_right_m=0.0,
        eaves_perimeter_m=eaves_perimeter,
        apex_height_m=apex_height,
        faces=[],
        skeleton_segments=classified,
        notes=[],
    )
