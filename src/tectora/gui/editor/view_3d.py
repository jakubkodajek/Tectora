# -*- coding: utf-8 -*-
"""3D roof preview — QPainter axonometric/perspective rendering (G7).

Pure projection math lives in module-level functions so unit-tests can import
them without a running QApplication.
"""
from __future__ import annotations

import math
from dataclasses import dataclass
from typing import Optional

from PySide6.QtCore import QPointF, Qt
from PySide6.QtGui import (
    QBrush,
    QColor,
    QFont,
    QMouseEvent,
    QPainter,
    QPen,
    QPolygonF,
    QWheelEvent,
)
from PySide6.QtWidgets import QSizePolicy, QWidget

from tectora.core.result import RoofResult

# ---------------------------------------------------------------------------
# Background / edge colours for the dark canvas
# ---------------------------------------------------------------------------
_BG_COLOR = QColor("#1f2430")
_EDGE_COLOR = QColor(220, 220, 220, 200)
_PLACEHOLDER_COLOR = QColor(80, 90, 110)

# Face fill colours (light, translucent)
_FACE_LIGHT = QColor(200, 215, 240, 170)
_FACE_MID = QColor(170, 185, 210, 150)
_FACE_DARK = QColor(130, 145, 170, 130)

# Wall colours
_WALL_FILL = QColor(0xD9, 0xDD, 0xE3, 200)   # #d9dde3, slightly translucent
_WALL_EDGE = QColor(0xB0, 0xB5, 0xBD, 220)   # slightly darker edge


# ---------------------------------------------------------------------------
# Pure math — unit-testable, no Qt needed
# ---------------------------------------------------------------------------

def _rotate_point(
    x: float,
    y: float,
    z: float,
    azimuth: float,
    elevation: float,
) -> tuple[float, float, float]:
    """Rotate a 3D point by azimuth (yaw around Z) then elevation (tilt around X).

    Returns the rotated (rx, ry, rz) triple.
    """
    # Yaw: rotate around Z-axis
    ca, sa = math.cos(azimuth), math.sin(azimuth)
    x1 = x * ca - y * sa
    y1 = x * sa + y * ca
    z1 = z
    # Elevation: tilt so +z (height) maps to screen-up, -z (walls) downward.
    ce, se = math.cos(elevation), math.sin(elevation)
    x2 = x1
    y2 = z1 * ce - y1 * se   # screen-up component
    z2 = y1 * ce + z1 * se   # depth (toward viewer) for painter's sort
    return x2, y2, z2


def project_point(
    pt3d: tuple[float, float, float],
    azimuth: float,
    elevation: float,
    scale: float,
    origin: tuple[float, float],
) -> tuple[float, float]:
    """Project a single 3D point to 2D screen coordinates.

    Args:
        pt3d:      (x, y, z) world-space point (already centred at origin).
        azimuth:   Horizontal rotation in radians.
        elevation: Vertical tilt in radians (positive = looking from above).
        scale:     Pixels per metre.
        origin:    (ox, oy) screen-space centre.

    Returns:
        (sx, sy) screen-space coordinates.
    """
    x, y, z = pt3d
    rx, ry, _rz = _rotate_point(x, y, z, azimuth, elevation)
    sx = origin[0] + rx * scale
    sy = origin[1] - ry * scale   # screen Y is inverted
    return sx, sy


def _depth_of_point(
    pt3d: tuple[float, float, float],
    azimuth: float,
    elevation: float,
) -> float:
    """Return the depth (eye-space Z) for back-to-front sorting."""
    x, y, z = pt3d
    _rx, _ry, rz = _rotate_point(x, y, z, azimuth, elevation)
    return rz   # larger rz = closer to viewer


# ---------------------------------------------------------------------------
# Face shading helpers
# ---------------------------------------------------------------------------

def _face_normal(vertices: list[tuple[float, float, float]]) -> tuple[float, float, float]:
    """Compute unit normal of a face from its first three vertices."""
    if len(vertices) < 3:
        return (0.0, 0.0, 1.0)
    ax, ay, az = vertices[0]
    bx, by, bz = vertices[1]
    cx, cy, cz = vertices[2]
    ux, uy, uz = bx - ax, by - ay, bz - az
    vx, vy, vz = cx - ax, cy - ay, cz - az
    nx = uy * vz - uz * vy
    ny = uz * vx - ux * vz
    nz = ux * vy - uy * vx
    length = math.sqrt(nx * nx + ny * ny + nz * nz)
    if length == 0:
        return (0.0, 0.0, 1.0)
    return (nx / length, ny / length, nz / length)


def _shade_color(normal: tuple[float, float, float]) -> QColor:
    """Return a face fill QColor based on how much the normal faces the virtual light."""
    # Light direction: slightly from front-left, elevated (normalised)
    lx, ly, lz = 0.4, -0.3, 0.85
    ll = math.sqrt(lx * lx + ly * ly + lz * lz)
    lx, ly, lz = lx / ll, ly / ll, lz / ll
    dot = max(0.0, normal[0] * lx + normal[1] * ly + normal[2] * lz)
    # Interpolate between dark and light face colours
    t = 0.35 + 0.65 * dot
    r = int(130 + (200 - 130) * t)
    g = int(145 + (215 - 145) * t)
    b = int(170 + (240 - 170) * t)
    return QColor(r, g, b, 160)


# ---------------------------------------------------------------------------
# Widget
# ---------------------------------------------------------------------------

@dataclass
class _ProjectedFace:
    polygon: QPolygonF
    color: QColor
    depth: float  # average depth (for painter's algorithm; largest = draw last / on top)


class Roof3DView(QWidget):
    """3D roof view with mouse rotation and scroll-wheel zoom.

    Public API:
        set_result(result)   — update the displayed RoofResult (None clears).
        reset_view()         — restore default azimuth/elevation/zoom.
    """

    def __init__(self, parent: Optional[QWidget] = None) -> None:
        super().__init__(parent)
        self.setMinimumSize(200, 150)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)
        self.setMouseTracking(False)

        self._result: Optional[RoofResult] = None

        # Geometry set via set_geometry (list of (verts, is_wall) tuples)
        # Each entry: (vertices: list[tuple[float,float,float]], is_wall: bool)
        self._geom_faces: list[tuple[list[tuple[float, float, float]], bool]] = []

        # View state — default: gentle bird's-eye perspective
        self._azimuth: float = math.radians(-35)
        self._elevation: float = math.radians(28)
        self._zoom: float = 1.0
        self._last_mouse: Optional[QPointF] = None

    # ------------------------------------------------------------------
    # Public API
    # ------------------------------------------------------------------

    def set_result(self, result: Optional[RoofResult]) -> None:
        """Display *result*; pass None to show the empty placeholder.

        For backwards compatibility this still works; the editor now calls
        set_geometry() which also stores the result via this path.
        """
        self._result = result
        self.update()

    def set_geometry(self, footprint, pitch_deg: float, wall_height: float, overhang: float = 0.0) -> None:
        """Compute and store roof + wall geometry, then repaint.

        Args:
            footprint:   A Footprint instance (or None to clear).
            pitch_deg:   Roof pitch in degrees.
            wall_height: Wall height in metres (walls go down from z=0).
            overhang:    Eave overhang in metres.  Walls stay on the original
                         footprint; the roof is built on the offset polygon so
                         the eaves extend beyond the wall tops.
        """
        if footprint is None:
            self._geom_faces = []
            self._result = None
            self.update()
            return
        self._geom_faces = self._geometry_for(footprint, pitch_deg, wall_height, overhang)
        self.update()

    def set_design(self, items) -> None:
        """Render an entire multi-building design.

        *items* is an iterable of ``(footprint, pitch_deg, wall_height,
        overhang)`` tuples — one per building, optionally with a 5th element
        ``roof_faces`` (precomputed FaceResults, e.g. a half-hip) used verbatim
        instead of rebuilding the roof. Buildings with ``footprint is None`` are
        skipped. All buildings share one scene.
        """
        geom: list[tuple[list[tuple[float, float, float]], bool]] = []
        for item in items:
            footprint, pitch_deg, wall_height, overhang = item[0], item[1], item[2], item[3]
            roof_faces = item[4] if len(item) > 4 else None
            if footprint is None:
                continue
            geom.extend(self._geometry_for(
                footprint, pitch_deg, wall_height, overhang, roof_faces))
        self._geom_faces = geom
        self._result = None
        self.update()

    def _geometry_for(
        self, footprint, pitch_deg: float, wall_height: float, overhang: float,
        roof_faces=None,
    ) -> list[tuple[list[tuple[float, float, float]], bool]]:
        """Build the (vertices, is_wall) faces for ONE building.

        The drawn polygon is the ROOF eave outline, so the roof is built on it
        directly. The overhang insets the WALLS inward (the building sits under
        the eave). Gable walls work at any overhang. If *roof_faces* is given
        (e.g. a half-hip computed by the exact calculator), they are used as the
        roof verbatim and gable walls are skipped.
        """
        from tectora.core.roof_faces import build_faces

        use_provided = roof_faces is not None and len(roof_faces) > 0
        faces = roof_faces if use_provided else build_faces(footprint, pitch_deg)

        geom: list[tuple[list[tuple[float, float, float]], bool]] = []

        # Walls sit INWARD of the eave by the overhang.
        wall_verts = [(float(x), float(y)) for x, y in footprint.vertices]
        if overhang > 1e-9:
            from tectora.core.footprint import is_simple_polygon
            from tectora.core.poly_offset import offset_polygon
            inset = offset_polygon(list(footprint.vertices), -overhang)
            if len(inset) == len(wall_verts) and is_simple_polygon(inset):
                wall_verts = [(float(x), float(y)) for x, y in inset]

        n = len(wall_verts)
        for i in range(n):
            ax, ay = wall_verts[i]
            bx, by = wall_verts[(i + 1) % n]
            quad: list[tuple[float, float, float]] = [
                (ax, ay, 0.0),
                (bx, by, 0.0),
                (bx, by, -wall_height),
                (ax, ay, -wall_height),
            ]
            geom.append((quad, True))  # True = wall

        # Roof faces (on the drawn polygon)
        for face in faces:
            geom.append((list(face.vertices), False))  # False = roof

        # Gable walls (štíty) — vertical triangles at valid gable wing-ends.
        # Skipped when explicit roof faces are supplied (e.g. a half-hip already
        # encodes its end geometry, with no full vertical gable).
        if not use_provided:
            from tectora.core.gable import GableNotSupported, gable_wall_triangles
            try:
                for tri in gable_wall_triangles(footprint, pitch_deg):
                    geom.append((list(tri), True))  # vertical štít = a wall face
            except GableNotSupported:
                pass  # invalid gable (shouldn't happen; 2D validates) — just skip

        return geom

    def reset_view(self) -> None:
        """Reset rotation and zoom to defaults."""
        self._azimuth = math.radians(-35)
        self._elevation = math.radians(28)
        self._zoom = 1.0
        self.update()

    # ------------------------------------------------------------------
    # Mouse / wheel interaction
    # ------------------------------------------------------------------

    def mousePressEvent(self, event: QMouseEvent) -> None:
        if event.button() == Qt.MouseButton.LeftButton:
            self._last_mouse = event.position()

    def mouseMoveEvent(self, event: QMouseEvent) -> None:
        if self._last_mouse is None:
            return
        dx = event.position().x() - self._last_mouse.x()
        dy = event.position().y() - self._last_mouse.y()
        self._azimuth -= dx * 0.01
        self._elevation += dy * 0.01
        # Clamp elevation to avoid flipping
        self._elevation = max(math.radians(-5), min(math.radians(85), self._elevation))
        self._last_mouse = event.position()
        self.update()

    def mouseReleaseEvent(self, event: QMouseEvent) -> None:
        if event.button() == Qt.MouseButton.LeftButton:
            self._last_mouse = None

    def wheelEvent(self, event: QWheelEvent) -> None:
        steps = event.angleDelta().y() / 120
        self._zoom *= 1.15 ** steps
        self._zoom = max(0.2, min(self._zoom, 6.0))
        self.update()

    # ------------------------------------------------------------------
    # Painting
    # ------------------------------------------------------------------

    def paintEvent(self, event) -> None:  # noqa: N802
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)

        # Dark background
        painter.fillRect(self.rect(), _BG_COLOR)

        # Prefer set_geometry geometry if available
        if self._geom_faces:
            self._draw_geom_scene(painter)
            return

        if self._result is None or not self._result.faces:
            self._draw_placeholder(painter)
            return

        self._draw_scene(painter)

    def _draw_placeholder(self, painter: QPainter) -> None:
        font = QFont("Segoe UI", 11)
        painter.setFont(font)
        painter.setPen(_PLACEHOLDER_COLOR)
        painter.drawText(
            self.rect(),
            Qt.AlignmentFlag.AlignCenter,
            "3D náhled",
        )

    def _draw_scene(self, painter: QPainter) -> None:
        result = self._result
        assert result is not None

        faces = result.faces
        if not faces:
            self._draw_placeholder(painter)
            return

        # Compute the bounding box of all vertices to auto-centre and auto-scale
        all_verts = [v for f in faces for v in f.vertices]
        xs = [v[0] for v in all_verts]
        ys = [v[1] for v in all_verts]
        zs = [v[2] for v in all_verts]

        cx = (min(xs) + max(xs)) / 2
        cy = (min(ys) + max(ys)) / 2
        cz = (min(zs) + max(zs)) / 2

        span = max(
            max(xs) - min(xs),
            max(ys) - min(ys),
            (max(zs) - min(zs)) * 2.0,  # height is typically smaller; amplify
            1.0,
        )

        view_w, view_h = self.width(), self.height()
        scale = min(view_w, view_h) / (span * 1.5) * self._zoom
        origin = (view_w / 2, view_h / 2 + view_h * 0.05)

        projected: list[_ProjectedFace] = []

        for face in faces:
            if len(face.vertices) < 3:
                continue

            centred = [(v[0] - cx, v[1] - cy, v[2] - cz) for v in face.vertices]
            screen_pts = [
                project_point(v, self._azimuth, self._elevation, scale, origin)
                for v in centred
            ]
            poly = QPolygonF([QPointF(sx, sy) for sx, sy in screen_pts])

            # Average depth for painter's algorithm (smaller rz = further away)
            avg_depth = sum(
                _depth_of_point(v, self._azimuth, self._elevation)
                for v in centred
            ) / len(centred)

            normal = _face_normal(face.vertices)
            color = _shade_color(normal)
            projected.append(_ProjectedFace(poly, color, avg_depth))

        # Sort furthest-first (smallest depth drawn first)
        projected.sort(key=lambda f: f.depth)

        edge_pen = QPen(_EDGE_COLOR)
        edge_pen.setWidthF(1.0)

        for pf in projected:
            painter.setBrush(QBrush(pf.color))
            painter.setPen(edge_pen)
            painter.drawPolygon(pf.polygon)

    def _draw_geom_scene(self, painter: QPainter) -> None:
        """Draw the combined wall + roof geometry stored in _geom_faces."""
        geom = self._geom_faces
        if not geom:
            self._draw_placeholder(painter)
            return

        # Collect all vertices to compute bounding box
        all_verts = [v for (verts, _is_wall) in geom for v in verts]
        xs = [v[0] for v in all_verts]
        ys = [v[1] for v in all_verts]
        zs = [v[2] for v in all_verts]

        cx = (min(xs) + max(xs)) / 2
        cy = (min(ys) + max(ys)) / 2
        cz = (min(zs) + max(zs)) / 2

        span = max(
            max(xs) - min(xs),
            max(ys) - min(ys),
            (max(zs) - min(zs)) * 1.5,
            1.0,
        )

        view_w, view_h = self.width(), self.height()
        scale = min(view_w, view_h) / (span * 1.6) * self._zoom
        origin = (view_w / 2, view_h / 2 + view_h * 0.05)

        projected: list[_ProjectedFace] = []

        for (face_verts, is_wall) in geom:
            if len(face_verts) < 3:
                continue

            centred = [(v[0] - cx, v[1] - cy, v[2] - cz) for v in face_verts]
            screen_pts = [
                project_point(v, self._azimuth, self._elevation, scale, origin)
                for v in centred
            ]
            poly = QPolygonF([QPointF(sx, sy) for sx, sy in screen_pts])

            avg_depth = sum(
                _depth_of_point(v, self._azimuth, self._elevation)
                for v in centred
            ) / len(centred)

            if is_wall:
                color = _WALL_FILL
            else:
                normal = _face_normal(face_verts)
                color = _shade_color(normal)

            projected.append(_ProjectedFace(poly, color, avg_depth))

        # Sort furthest-first
        projected.sort(key=lambda f: f.depth)

        roof_edge_pen = QPen(_EDGE_COLOR)
        roof_edge_pen.setWidthF(1.0)
        wall_edge_pen = QPen(_WALL_EDGE)
        wall_edge_pen.setWidthF(0.8)

        for i, pf in enumerate(projected):
            # Determine if this is a wall face by checking the original entry
            # We need to track wall vs roof — re-derive from position in list
            painter.setBrush(QBrush(pf.color))
            # Use wall edge pen for wall-coloured faces, roof pen otherwise
            if pf.color == _WALL_FILL:
                painter.setPen(wall_edge_pen)
            else:
                painter.setPen(roof_edge_pen)
            painter.drawPolygon(pf.polygon)
