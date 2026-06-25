# -*- coding: utf-8 -*-
"""Pure geometric helpers for the 2D canvas. No Qt imports — fully testable."""
from __future__ import annotations

import math
from typing import Optional


def screen_to_model(
    px: float,
    py: float,
    origin_px: tuple[float, float],
    scale: float,
) -> tuple[float, float]:
    """Convert screen pixel coordinates to model coordinates (metres).

    ``origin_px`` is the pixel position of model point (0, 0).
    ``scale`` is pixels per metre.

    Screen Y axis points *down*, model Y axis points *up*, so the Y
    component is negated on conversion.
    """
    ox, oy = origin_px
    mx = (px - ox) / scale
    my = -(py - oy) / scale
    return mx, my


def model_to_screen(
    mx: float,
    my: float,
    origin_px: tuple[float, float],
    scale: float,
) -> tuple[float, float]:
    """Convert model coordinates (metres) to screen pixel coordinates.

    Inverse of :func:`screen_to_model`.
    """
    ox, oy = origin_px
    px = ox + mx * scale
    py = oy - my * scale
    return px, py


def snap_to_grid(
    mx: float,
    my: float,
    step: float,
) -> tuple[float, float]:
    """Round model coordinates to the nearest grid step (in metres)."""
    sx = round(mx / step) * step
    sy = round(my / step) * step
    return sx, sy


def hit_test_node(
    px: float,
    py: float,
    node_screen_pts: list[tuple[float, float]],
    radius: float,
) -> Optional[int]:
    """Return the index of the first node within *radius* pixels of (px, py),
    or ``None`` if no node is close enough.
    """
    for i, (nx, ny) in enumerate(node_screen_pts):
        if math.hypot(px - nx, py - ny) <= radius:
            return i
    return None


def point_in_polygon(
    px: float,
    py: float,
    polygon_screen_pts: list[tuple[float, float]],
) -> bool:
    """Ray-casting point-in-polygon test (screen space). Edges count as inside-ish."""
    n = len(polygon_screen_pts)
    if n < 3:
        return False
    inside = False
    j = n - 1
    for i in range(n):
        xi, yi = polygon_screen_pts[i]
        xj, yj = polygon_screen_pts[j]
        if ((yi > py) != (yj > py)) and (
            px < (xj - xi) * (py - yi) / (yj - yi + 1e-12) + xi
        ):
            inside = not inside
        j = i
    return inside


def point_segment_distance(
    px: float,
    py: float,
    a: tuple[float, float],
    b: tuple[float, float],
) -> float:
    """Shortest distance from point (px, py) to segment a–b (screen space)."""
    ax, ay = a
    bx, by = b
    dx, dy = bx - ax, by - ay
    seg2 = dx * dx + dy * dy
    if seg2 < 1e-12:
        return math.hypot(px - ax, py - ay)
    t = ((px - ax) * dx + (py - ay) * dy) / seg2
    t = max(0.0, min(1.0, t))
    fx, fy = ax + t * dx, ay + t * dy
    return math.hypot(px - fx, py - fy)


def hit_test_edge(
    px: float,
    py: float,
    edge_segments_screen: list[tuple[tuple[float, float], tuple[float, float]]],
    tol: float,
) -> Optional[int]:
    """Return the index of the first edge segment whose perpendicular distance
    to (px, py) is within *tol* pixels and whose projection falls between the
    endpoints, or ``None`` if none qualifies.
    """
    for i, (a, b) in enumerate(edge_segments_screen):
        ax, ay = a
        bx, by = b
        dx, dy = bx - ax, by - ay
        seg_len_sq = dx * dx + dy * dy
        if seg_len_sq < 1e-12:
            # Degenerate segment: check endpoint distance
            if math.hypot(px - ax, py - ay) <= tol:
                return i
            continue
        # Parameter t of the projection of (px, py) onto the segment
        t = ((px - ax) * dx + (py - ay) * dy) / seg_len_sq
        if t < 0.0 or t > 1.0:
            continue
        # Foot of perpendicular
        fx = ax + t * dx
        fy = ay + t * dy
        if math.hypot(px - fx, py - fy) <= tol:
            return i
    return None
