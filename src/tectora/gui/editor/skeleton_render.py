# -*- coding: utf-8 -*-
"""Pure (no-Qt) helpers for rendering the roof skeleton on the 2D canvas.

Public functions:

  classify_skeleton(result, tol=1e-6)
      Returns interior skeleton segments with kind labels and 3D length.
      Each item: ``(kind, (x1, y1), (x2, y2), length3d)``

  face_area_labels(result)
      Returns ``[(cx, cy, area_m2), ...]`` for each face in the calculator frame.

  fmt_len(m) / fmt_area(m2)
      Czech-locale number formatters.

  map_calc_point(cx, cy, bbox)
      Maps a point from the calculator's frame into the footprint's model frame.
"""
from __future__ import annotations

import math
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from tectora.core.result import RoofResult

# ---------------------------------------------------------------------------
# Czech-locale formatters
# ---------------------------------------------------------------------------

def fmt_len(m: float) -> str:
    """Format a length in metres with Czech comma decimal, e.g. ``"4,00 m"``."""
    return f"{m:.2f}".replace(".", ",") + " m"


def fmt_area(m2: float) -> str:
    """Format an area in m² with Czech comma decimal, e.g. ``"4,62 m²"``."""
    return f"{m2:.2f}".replace(".", ",") + " m²"


# ---------------------------------------------------------------------------
# classify_skeleton
# ---------------------------------------------------------------------------

def classify_skeleton(
    result: "RoofResult",
    tol: float = 1e-6,
) -> list[tuple[str, tuple[float, float], tuple[float, float], float]]:
    """Classify interior skeleton edges of *result* as ridge or hip.

    Algorithm:
      1. Collect every edge of every face (consecutive vertices; polygon closed).
      2. Key each undirected edge by the frozenset of its two endpoints
         rounded to 6 decimal places.
      3. Count how many faces share each edge.
      4. For edges shared by >= 2 faces:
         - both endpoints have z > tol AND |z1 - z2| < tol → "ridge"
         - otherwise → "hip"

    Returns a list of ``(kind, (x1, y1), (x2, y2), length3d)`` — z coordinates
    are dropped from the 2D positions but used for the 3D Euclidean length.
    Only the shared (interior) edges are returned.
    """
    # edge_key → {"p1": ..., "p2": ..., "count": int}
    edge_data: dict[frozenset, dict] = {}

    for face in result.faces:
        verts = face.vertices  # list of (x, y, z)
        n = len(verts)
        for i in range(n):
            p1 = verts[i]
            p2 = verts[(i + 1) % n]
            # Round to 6 decimals for stable keying
            r1 = tuple(round(c, 6) for c in p1)
            r2 = tuple(round(c, 6) for c in p2)
            key = frozenset({r1, r2})
            if key in edge_data:
                edge_data[key]["count"] += 1
            else:
                edge_data[key] = {"p1": p1, "p2": p2, "count": 1}

    segments: list[tuple[str, tuple[float, float], tuple[float, float], float]] = []

    for info in edge_data.values():
        if info["count"] < 2:
            continue  # boundary edge (eave / gable perimeter)
        p1, p2 = info["p1"], info["p2"]
        x1, y1, z1 = p1
        x2, y2, z2 = p2
        if z1 > tol and z2 > tol and abs(z1 - z2) < tol:
            kind = "ridge"
        else:
            kind = "hip"
        # True 3D Euclidean distance
        length3d = math.sqrt((x2 - x1) ** 2 + (y2 - y1) ** 2 + (z2 - z1) ** 2)
        segments.append((kind, (x1, y1), (x2, y2), length3d))

    return segments


# ---------------------------------------------------------------------------
# face_area_labels
# ---------------------------------------------------------------------------

def face_area_labels(
    result: "RoofResult",
) -> list[tuple[float, float, float]]:
    """Return ``(centroid_x, centroid_y, area_m2)`` for each face.

    Centroid is the average of the face's x, y vertices (in the calculator
    frame).  Area is taken directly from :attr:`FaceResult.area_m2`.
    """
    labels: list[tuple[float, float, float]] = []
    for face in result.faces:
        verts = face.vertices  # list of (x, y, z)
        n = len(verts)
        cx = sum(v[0] for v in verts) / n
        cy = sum(v[1] for v in verts) / n
        labels.append((cx, cy, face.area_m2))
    return labels


# ---------------------------------------------------------------------------
# map_calc_point
# ---------------------------------------------------------------------------

def map_calc_point(
    cx: float,
    cy: float,
    bbox: tuple[float, float, float, float],
) -> tuple[float, float]:
    """Map a point from the calculator frame into the footprint model frame.

    The calculator places the longer dimension along its x-axis (L) and the
    shorter along y (W).  The footprint's bounding box ``bbox = (x0, y0, x1, y1)``
    may have either orientation:

    - If the bbox is wider (w >= d): length runs along footprint x → identity
      translate: ``(x0 + cx, y0 + cy)``.
    - If the bbox is taller (d > w): length runs along footprint y → swap
      x/y: ``(x0 + cy, y0 + cx)``.
    """
    x0, y0, x1, y1 = bbox
    w = x1 - x0
    d = y1 - y0
    if w >= d:
        return (x0 + cx, y0 + cy)
    else:
        return (x0 + cy, y0 + cx)
