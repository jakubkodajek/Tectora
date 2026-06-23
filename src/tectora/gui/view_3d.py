"""3D roof preview — interactive QPainter axonometric rendering.

OpenGL and external 3D libraries are intentionally avoided — roof models are
simple, and Qt's CPU polygon rasterisation is fast enough while giving us full
control over style (lighting, edges, labels).
"""
from __future__ import annotations

import math
from dataclasses import dataclass

from PySide6.QtCore import QPointF, QRect, Qt
from PySide6.QtGui import (
    QBrush,
    QColor,
    QFont,
    QLinearGradient,
    QMouseEvent,
    QPainter,
    QPainterPath,
    QPen,
    QPolygonF,
    QWheelEvent,
)
from PySide6.QtWidgets import QWidget

from tectora.core.result import RoofResult
from tectora.gui import style


@dataclass
class _ProjectedFace:
    polygon: QPolygonF
    color: QColor
    depth: float          # mean z-coordinate after rotation (for painter's algorithm)
    normal_z: float       # for shading
    label: str


class Roof3DView(QWidget):
    """3D roof view with mouse rotation and scroll-wheel zoom."""

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setMinimumHeight(280)
        self.setMouseTracking(False)
        self.setSizePolicy(self.sizePolicy().horizontalPolicy(), self.sizePolicy().verticalPolicy())

        self._result: RoofResult | None = None
        # View angles (radians). Default = gentle bird's-eye perspective.
        self._yaw = math.radians(-35)    # rotation around z-axis (horizontal spin)
        self._pitch = math.radians(28)   # tilt (looking "from above")
        self._zoom = 1.0
        self._last_mouse: QPointF | None = None

    # ---------- public API ----------
    def set_result(self, result: RoofResult | None) -> None:
        self._result = result
        self.update()

    def reset_view(self) -> None:
        self._yaw = math.radians(-35)
        self._pitch = math.radians(28)
        self._zoom = 1.0
        self.update()

    # ---------- events ----------
    def mousePressEvent(self, event: QMouseEvent) -> None:
        if event.button() == Qt.MouseButton.LeftButton:
            self._last_mouse = event.position()

    def mouseMoveEvent(self, event: QMouseEvent) -> None:
        if self._last_mouse is None:
            return
        dx = event.position().x() - self._last_mouse.x()
        dy = event.position().y() - self._last_mouse.y()
        self._yaw -= dx * 0.01
        self._pitch += dy * 0.01
        # clamp pitch so we don't flip over the pole
        self._pitch = max(math.radians(-5), min(math.radians(85), self._pitch))
        self._last_mouse = event.position()
        self.update()

    def mouseReleaseEvent(self, event: QMouseEvent) -> None:
        if event.button() == Qt.MouseButton.LeftButton:
            self._last_mouse = None

    def wheelEvent(self, event: QWheelEvent) -> None:
        steps = event.angleDelta().y() / 120
        self._zoom *= 1.15 ** steps
        self._zoom = max(0.3, min(self._zoom, 5.0))
        self.update()

    # ---------- rendering ----------
    def paintEvent(self, event):  # noqa: N802
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)

        # background — vertical sky gradient
        sky = QLinearGradient(0, 0, 0, self.height())
        sky.setColorAt(0.0, QColor("#E9F1FA"))
        sky.setColorAt(1.0, QColor("#FFFFFF"))
        painter.fillRect(self.rect(), QBrush(sky))

        if self._result is None:
            painter.setPen(QColor(style.TEXT_MUTED))
            painter.setFont(QFont("Segoe UI", 11))
            painter.drawText(
                self.rect(),
                Qt.AlignmentFlag.AlignCenter,
                "3D náhled\nVyplň parametry a klikni na „Provést výpočet“",
            )
            return

        self._draw_scene(painter)

    # ---------- projection ----------
    def _project(self, x: float, y: float, z: float) -> tuple[float, float, float]:
        """Return (sx, sy, depth) — screen coords plus depth for painter's algorithm."""
        cy, sy_ = math.cos(self._yaw), math.sin(self._yaw)
        cp, sp = math.cos(self._pitch), math.sin(self._pitch)

        # first rotate around z-axis (yaw)
        x1 = x * cy - y * sy_
        y1 = x * sy_ + y * cy
        z1 = z
        # then rotate around x-axis (pitch) — tilt from above
        x2 = x1
        y2 = y1 * cp - z1 * sp
        z2 = y1 * sp + z1 * cp

        return x2, y2, -z2  # -z2 as "into the distance" (larger = closer to eye)

    def _draw_scene(self, painter: QPainter) -> None:
        assert self._result is not None
        result = self._result
        spec = result.spec

        L = spec.roof_length
        W = spec.roof_width
        h = result.apex_height_m

        # shift model centre to (0,0,0)
        cx, cy, cz = L / 2, W / 2, h / 2

        # compute scale so the model fits inside the widget
        # projected corners give a bounding box; approximate as max(L,W) + height
        approx_size = max(L, W) + h * 1.4
        view_w, view_h = self.width(), self.height()
        scale = min(view_w, view_h) / (approx_size * 1.3) * self._zoom
        center_x, center_y = view_w / 2, view_h / 2 + 20

        def proj(p):
            x, y, z = p
            px, py, depth = self._project(x - cx, y - cy, z - cz)
            return QPointF(center_x + px * scale, center_y - py * scale), depth

        # 1) GROUND (just as a footprint shadow)
        ground_poly = QPolygonF([
            proj((0, 0, 0))[0],
            proj((L, 0, 0))[0],
            proj((L, W, 0))[0],
            proj((0, W, 0))[0],
        ])
        painter.setPen(Qt.PenStyle.NoPen)
        painter.setBrush(QColor(style.GROUND[0], style.GROUND[1], style.GROUND[2], 120))
        painter.drawPolygon(ground_poly)

        # 2) WALLS (4 sides of the building below the roof — eaves sit at z=0 in our
        # model, but for visualisation we add an imaginary wall so the house is visible.
        # Wall height `wall_h` = max(2.5 m, h * 0.6)
        wall_h = max(2.5, h * 0.6)
        # Shift the roof up by wall_h so it sits on top of the walls
        def lift(p):
            return (p[0], p[1], p[2] + wall_h)

        wall_faces = [
            # front (y=0)
            ((0, 0, 0), (L, 0, 0), (L, 0, wall_h), (0, 0, wall_h)),
            # right (x=L)
            ((L, 0, 0), (L, W, 0), (L, W, wall_h), (L, 0, wall_h)),
            # back (y=W)
            ((L, W, 0), (0, W, 0), (0, W, wall_h), (L, W, wall_h)),
            # left (x=0)
            ((0, W, 0), (0, 0, 0), (0, 0, wall_h), (0, W, wall_h)),
        ]

        # 3) Prepare all FACES (walls + roof) with depth for painter's algorithm
        faces: list[_ProjectedFace] = []

        for verts in wall_faces:
            proj_verts = [proj(v) for v in verts]
            poly = QPolygonF([p[0] for p in proj_verts])
            depth = sum(p[1] for p in proj_verts) / len(proj_verts)

            # compute normal for shading (whether the face is front-facing)
            v0, v1, v2 = verts[0], verts[1], verts[2]
            nrm = _normal(v0, v1, v2)
            # whether the face is turned towards the eye — simple heuristic based on yaw
            visibility = _wall_visibility(v0, v1, self._yaw, self._pitch, (cx, cy))

            base = QColor(*style.WALL) if visibility > 0 else QColor(*style.WALL_SHADOW)
            faces.append(_ProjectedFace(poly, base, depth, nrm[2], ""))

        # roof planes
        for f in result.faces:
            lifted = [lift(v) for v in f.vertices]
            proj_verts = [proj(v) for v in lifted]
            poly = QPolygonF([p[0] for p in proj_verts])
            depth = sum(p[1] for p in proj_verts) / len(proj_verts)

            nrm = _normal(lifted[0], lifted[1], lifted[2])
            # zenith shading — light from north-east, high up
            light = _max(0.35, _dot(nrm, _normalize((0.4, -0.3, 0.85))))
            base = _shade_roof_color(light)
            faces.append(_ProjectedFace(poly, base, depth, nrm[2], f.name))

        # sort by depth — furthest first (painter's algorithm)
        faces.sort(key=lambda f: f.depth)

        # 4) Draw
        edge_pen = QPen(QColor(*style.EDGE))
        edge_pen.setWidthF(1.2)
        for face in faces:
            painter.setBrush(QBrush(face.color))
            painter.setPen(edge_pen)
            painter.drawPolygon(face.polygon)

        # 5) Key-edge labels (optional, simple)
        self._draw_ridge_marker(painter, proj, lift, result, spec)

        # 6) Compass rose in corner + HUD info
        self._draw_compass(painter)
        self._draw_hud(painter, result)

    def _draw_ridge_marker(self, painter, proj, lift, result, spec):
        """Draw the ridge as a bold line."""
        if result.ridge_length_m <= 0:
            return
        from tectora.core.spec import RoofType
        L, W = spec.roof_length, spec.roof_width
        h = result.apex_height_m

        if spec.type == RoofType.GABLE:
            # apex y-position
            y_apex = result.faces[0].vertices[2][1]  # z faces front, vertex (L, y_apex, h)
            p0 = proj(lift((0, y_apex, h)))[0]
            p1 = proj(lift((L, y_apex, h)))[0]
        elif spec.type == RoofType.HIP:
            p0 = proj(lift((W / 2, W / 2, h)))[0]
            p1 = proj(lift((L - W / 2, W / 2, h)))[0]
        elif spec.type == RoofType.HALF_HIP:
            d = spec.half_hip_ratio * W / 2
            p0 = proj(lift((d, W / 2, h)))[0]
            p1 = proj(lift((L - d, W / 2, h)))[0]
        else:
            return

        pen = QPen(QColor(style.PRIMARY))
        pen.setWidthF(2.5)
        painter.setPen(pen)
        painter.drawLine(p0, p1)

    def _draw_compass(self, painter: QPainter) -> None:
        """Draw a compass rose in the top-left corner."""
        cx, cy, r = 40, 40, 22
        painter.setPen(QPen(QColor(style.TEXT_MUTED), 1))
        painter.setBrush(Qt.BrushStyle.NoBrush)
        painter.drawEllipse(QPointF(cx, cy), r, r)

        # north direction based on current yaw
        n_angle = self._yaw - math.pi / 2  # north = -y in our coordinate system
        nx = cx + math.sin(n_angle) * r
        ny = cy - math.cos(n_angle) * r
        path = QPainterPath()
        path.moveTo(cx, cy)
        path.lineTo(nx, ny)
        pen = QPen(QColor(style.PRIMARY), 2)
        painter.setPen(pen)
        painter.drawPath(path)
        painter.setFont(QFont("Segoe UI", 8, QFont.Weight.Bold))
        painter.setPen(QColor(style.PRIMARY))
        painter.drawText(int(nx) - 4, int(ny) - 4, "S")

    def _draw_hud(self, painter: QPainter, result: RoofResult) -> None:
        """Draw brief info in the top-right corner."""
        text = (
            f"{result.spec.type.cs_name}  •  "
            f"{result.spec.length:.1f} × {result.spec.width:.1f} m  •  "
            f"{result.spec.pitch:.0f}°"
        )
        painter.setFont(QFont("Segoe UI", 9))
        painter.setPen(QColor(style.TEXT_MUTED))
        rect = QRect(0, 8, self.width() - 12, 18)
        painter.drawText(rect, Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignTop, text)


# ---------- small helper functions ----------
def _normal(a, b, c):
    ax, ay, az = a
    bx, by, bz = b
    cx, cy, cz = c
    ux, uy, uz = bx - ax, by - ay, bz - az
    vx, vy, vz = cx - ax, cy - ay, cz - az
    nx = uy * vz - uz * vy
    ny = uz * vx - ux * vz
    nz = ux * vy - uy * vx
    return _normalize((nx, ny, nz))


def _normalize(v):
    n = math.sqrt(v[0] ** 2 + v[1] ** 2 + v[2] ** 2)
    if n == 0:
        return (0.0, 0.0, 0.0)
    return (v[0] / n, v[1] / n, v[2] / n)


def _dot(a, b):
    return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]


def _max(a, b):
    return a if a > b else b


def _shade_roof_color(light: float) -> QColor:
    """Interpolate between the shadow and highlight roof colours."""
    light = max(0.0, min(1.0, light))
    sh = style.ROOF_SHADOW
    hi = style.ROOF_HIGHLIGHT
    r = int(sh[0] + (hi[0] - sh[0]) * light)
    g = int(sh[1] + (hi[1] - sh[1]) * light)
    b = int(sh[2] + (hi[2] - sh[2]) * light)
    return QColor(r, g, b)


def _wall_visibility(p0, p1, yaw, pitch, center):
    """Determine whether the wall defined by edge p0->p1 faces the viewer."""
    # The wall is vertical — its normal lies in the XY plane
    # Viewer: direction from centre after rotation
    dx = math.sin(yaw) * math.cos(pitch)
    dy = -math.cos(yaw) * math.cos(pitch)

    # Outward wall normal — perpendicular to the edge, pointing away from the model centre
    ex = p1[0] - p0[0]
    ey = p1[1] - p0[1]
    # normal candidate (90° left rotation): (-ey, ex)
    nx = -ey
    ny = ex
    mx, my = (p0[0] + p1[0]) / 2, (p0[1] + p1[1]) / 2
    # flip if pointing inward
    if (mx - center[0]) * nx + (my - center[1]) * ny < 0:
        nx, ny = -nx, -ny
    # Dot product with viewer direction (negative = visible)
    return -(nx * dx + ny * dy)
