"""Miter-offset a CCW simple polygon outward by distance *d*.

For a CCW polygon the outward normal of edge v_i→v_{i+1} is the right-hand
perpendicular of the unit direction: n = (u.y, -u.x).

Each edge line is shifted outward by *d*, and the new vertex at corner *i* is
the intersection of the two adjacent shifted edge-lines.  If the two lines are
parallel (collinear edges), we fall back to the shifted midpoint of the shorter
segment, which is exact when d == 0 and sensible otherwise.
"""
from __future__ import annotations

from tectora.core.geom2d import (
    EPS,
    add,
    length,
    line_intersection,
    normalize,
    scale,
    sub,
)


def offset_polygon(polygon_ccw: list[tuple], d: float) -> list[tuple]:
    """Return a new polygon that is *polygon_ccw* expanded outward by *d*.

    Parameters
    ----------
    polygon_ccw:
        Vertices of a CCW simple polygon as a list of (x, y) float-like tuples.
    d:
        Outward offset distance in the same units as the vertex coordinates.
        *d = 0* returns a copy with float-coerced coordinates.

    Returns
    -------
    list[tuple[float, float]]
        Offset polygon vertices (same winding order, same length as input).
    """
    pts = [(float(x), float(y)) for x, y in polygon_ccw]
    n = len(pts)

    # Precompute per-edge outward normals and a shifted point on each edge.
    # For CCW polygon, outward normal of edge i→i+1 is (u.y, -u.x).
    normals: list[tuple] = []
    shifted_pts: list[tuple] = []

    for i in range(n):
        a = pts[i]
        b = pts[(i + 1) % n]
        edge = sub(b, a)
        try:
            u = normalize(edge)
        except ValueError:
            # Zero-length edge — use a placeholder normal; will be handled below.
            u = (1.0, 0.0)
        # Outward normal (CCW polygon → right-perpendicular = outward)
        normal = (u[1], -u[0])
        normals.append(normal)
        # A shifted point on this edge line (vertex a moved by d along normal)
        shifted_pts.append(add(a, scale(normal, d)))

    result: list[tuple] = []
    for i in range(n):
        # Edge i arrives at vertex i  (edge i goes from vertex i to vertex i+1)
        # Edge i-1 departs from vertex i (edge i-1 goes from vertex i-1 to vertex i)
        prev_i = (i - 1) % n

        # Line for edge (i-1): point = shifted_pts[prev_i], dir = normals[prev_i] rotated 90°
        # The edge direction is perpendicular to the outward normal.
        # For normal = (ny, -nx), the edge direction = (nx, ny) = (-normal[1], normal[0]).
        dir_prev = (-normals[prev_i][1], normals[prev_i][0])
        dir_curr = (-normals[i][1], normals[i][0])

        pt = line_intersection(
            shifted_pts[prev_i], dir_prev,
            shifted_pts[i],      dir_curr,
        )
        if pt is None:
            # Parallel edges: fall back to the offset of the original vertex.
            pt = add(pts[i], scale(normals[i], d))

        result.append((float(pt[0]), float(pt[1])))

    return result
