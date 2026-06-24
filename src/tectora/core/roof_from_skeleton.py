"""Build a RoofResult from the straight-skeleton of a Footprint.

SS4 — classify skeleton arcs into ridge/hip/valley, compute 3-D lengths,
aggregate totals, and validate against the exact calculator for rectangles.
"""
from __future__ import annotations

import math
from math import radians, tan, cos, sqrt

from tectora.core.footprint import Footprint
from tectora.core.gable import folded_skeleton
from tectora.core.result import RoofResult
from tectora.core.roof_faces import build_faces
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
    slope = tan(radians(pitch_deg))

    # -----------------------------------------------------------------------
    # Compute the hipped skeleton, then fold every `gable` edge (Stage 1).
    # `folded_skeleton` returns the modified node set / arcs / per-arc rake
    # flags / offset-height lookup / set of gabled edge indices, and raises
    # GableNotSupported for an invalid gable placement. build_roof and
    # build_faces share this helper so they agree on the folded geometry.
    # -----------------------------------------------------------------------
    folded = folded_skeleton(footprint)
    node_t = folded.node_t
    rake_keys = folded.rake_keys
    gable_edges = folded.gable_edges

    # Polygon vertices are eave-level (t = 0).
    poly_set: set[tuple] = {_round2(p) for p in poly}

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
    # Classify (folded) arcs.
    # -----------------------------------------------------------------------
    EPS = 1e-6

    classified: list[tuple] = []   # (kind, a3d, b3d)

    for (ax, ay), (bx, by) in folded.arcs:
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

        if frozenset((_round2(a2), _round2(b2))) in rake_keys:
            # Folded gable rake (A-M / B-M): a vertical-wall edge, not a hip.
            kind = "gable"
        elif a_is_poly or b_is_poly:
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
    gable_len = 0.0
    hip_corners: set = set()
    valley_corners: set = set()

    # Count hips/valleys as RAFTERS (one per polygon corner), not arc segments:
    # on non-axis-aligned shapes a rafter can be split into several arcs by
    # intermediate skeleton nodes, so we tally the distinct corner endpoints.
    for kind, a3, b3 in classified:
        seg = _len3d(a3, b3)
        if kind == "ridge":
            ridge_len += seg
        elif kind == "gable":
            gable_len += seg
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
    # reflex corner one valley — EXCEPT the convex endpoints of a gabled edge,
    # whose hip rafters have been folded into rakes. If the skeleton violates
    # this it is unreliable for this shape — fail loudly (RoofGenerationError is
    # a ValueError, so the GUI/CLI report it gracefully) rather than drawing a
    # wrong roof.
    gable_corners: set = set()
    for i in gable_edges:
        gable_corners.add(_round2(footprint.vertices[i]))
        gable_corners.add(_round2(footprint.vertices[(i + 1) % footprint.n]))
    n_reflex = len(reflex_pts)
    n_convex = footprint.n - n_reflex
    expected_hips = n_convex - len(gable_corners & (poly_set - reflex_pts))
    if hip_count != expected_hips or valley_count != n_reflex:
        raise RoofGenerationError(
            "Skeleton failed the structural check for this shape "
            f"(hips {hip_count}/{expected_hips}, valleys {valley_count}/{n_reflex})."
        )

    # -----------------------------------------------------------------------
    # Totals that don't depend on arc classification.
    # -----------------------------------------------------------------------
    footprint_area = _shoelace(poly)
    total_area = footprint_area / cos(radians(pitch_deg))

    # Eave perimeter excludes gable edges (the gable wall is not an eave).
    eaves_perimeter = sum(
        footprint.edge_length(i)
        for i in range(footprint.n)
        if i not in gable_edges
    )

    apex_height = max(
        (t * slope for t in node_t.values()),
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
        # The full straight-skeleton rake (štít) total goes in the left slot;
        # gable_edge_total_m then reports the correct sum. A later GUI task owns
        # splitting/displaying per-gable.
        gable_edge_left_m=gable_len,
        gable_edge_right_m=0.0,
        eaves_perimeter_m=eaves_perimeter,
        apex_height_m=apex_height,
        faces=build_faces(footprint, pitch_deg),
        skeleton_segments=classified,
        notes=[],
    )
