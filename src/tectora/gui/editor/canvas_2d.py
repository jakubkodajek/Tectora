# -*- coding: utf-8 -*-
"""Canvas2D — 2D drawing canvas for the roof footprint editor (G3).

Public model API (tested offscreen without mouse events):
  add_point(mx, my)        — append a vertex while drawing
  close_polygon()          — finalise polygon (requires ≥ 3 vertices)
  move_vertex(i, mx, my)   — reposition vertex i
  toggle_edge_type(i)      — flip edge i between "eave" / "gable"
  clear()                  — reset to empty drawing state
  footprint                — property: Footprint or None
  set_skeleton(data)       — G4 placeholder: store skeleton data

The widget is placed in editor.canvas_container by EditorView.
"""
from __future__ import annotations

from dataclasses import dataclass, field
from typing import TYPE_CHECKING, Any, Optional

from PySide6.QtCore import QPoint, QPointF, QRect, QSize, Qt
from PySide6.QtGui import (
    QColor,
    QFont,
    QPainter,
    QPen,
    QMouseEvent,
)
from PySide6.QtWidgets import QWidget
from PySide6.QtCore import Signal

from tectora.gui.editor.canvas_geometry import (
    hit_test_edge,
    hit_test_node,
    model_to_screen,
    point_in_polygon as _point_in_polygon,
    screen_to_model,
    snap_to_grid,
)
from tectora.gui.i18n import tr
from tectora.gui import theme
from tectora.gui.theme import ACCENT, BG, BORDER, BORDER_SOFT, CARD, COL, FAINT, MID, MUTED, TEXT

if TYPE_CHECKING:
    from tectora.gui.editor.editor_view import EditorView

# ---------------------------------------------------------------------------
# Layout / geometry constants
# ---------------------------------------------------------------------------

_GRID_STEP_M = 0.5          # minor grid lines every 0.5 m
_GRID_EMPHASIS_1M = 1.0     # medium emphasis every 1 m
_GRID_EMPHASIS_5M = 5.0     # strong emphasis every 5 m
_DEFAULT_SCALE = 60         # px / m  (sensible default for a 12x8 building)
_NODE_RADIUS = 6            # px, draggable node circles
_SNAP_INDICATOR_R = 5       # px, snap indicator circle
_CLOSE_SNAP_RADIUS = 12     # px — click within this to close polygon
_EDGE_TOL = 8               # px, edge hit tolerance
_LEGEND_W = 136             # px
_LEGEND_PAD = 10            # px
_LEGEND_ROW_H = 18          # px
_LEGEND_ROWS = 4            # unused; legend height derives from len(_LEGEND_ITEMS)

# Legend items: (i18n_key, COL_key)
_LEGEND_ITEMS = [
    ("ridge", "ridge"),
    ("hip", "hip"),
    ("valley", "valley"),
    ("eave", "eave"),
    ("gable", "gable"),
]


@dataclass
class _CanvasBuilding:
    """Per-building model state on the canvas (one drawn footprint + its roof).

    The canvas holds a list of these and an active index; the legacy
    single-building attributes (``_vertices`` etc.) delegate to the active one.
    """
    vertices: list = field(default_factory=list)     # list[tuple[float, float]]
    edge_types: list = field(default_factory=list)   # list[str]
    closed: bool = False
    skeleton_data: Any = None                         # RoofResult or None
    overhang: float = 0.0
    pitch: int = 35
    wall_height: float = 2.5
    name: str = ""


class Canvas2D(QWidget):
    """2D drawing canvas.  Pass the ``EditorView`` instance as *editor*."""

    # Emitted when a closed polygon is edited (toggle_edge_type / move_vertex)
    edited = Signal()
    # Emitted with the active index when the active building changes
    # (new_building / set_active / a successful select_building_at).
    active_changed = Signal(int)

    def __init__(self, editor: "EditorView"):
        super().__init__(editor.canvas_container)
        self._editor = editor
        self._lang = getattr(editor, "_lang", "cs")

        # ── Model state — list of buildings + active index ───────────────
        # The legacy single-building attributes (_vertices/_edge_types/_closed/
        # _skeleton_data/_overhang) are properties that delegate to the active
        # building, so existing single-building code keeps working unchanged.
        self._buildings: list[_CanvasBuilding] = [_CanvasBuilding()]
        self._active: int = 0

        # ── View state ───────────────────────────────────────────────────
        # Origin = pixel position of model (0, 0); widget centre + pan offset.
        self._scale: float = _DEFAULT_SCALE
        self._cursor_screen: Optional[tuple[float, float]] = None
        self._snap_pt: Optional[tuple[float, float]] = None  # snapped model pt

        # Pan offset (pixels) applied to the origin; dragged with the RIGHT button.
        self._pan: tuple[float, float] = (0.0, 0.0)
        self._pan_anchor: Optional[tuple[float, float, float, float]] = None

        # editPoints: currently dragged node index
        self._drag_idx: Optional[int] = None

        # Cursor shape
        self.setMouseTracking(True)
        self.setCursor(Qt.CursorShape.CrossCursor)
        self.setMinimumSize(QSize(200, 200))

    # ------------------------------------------------------------------
    # Active-building delegation (legacy single-building API)
    # ------------------------------------------------------------------

    @property
    def _b(self) -> _CanvasBuilding:
        """The active building (the one drawing/editing operates on)."""
        if not self._buildings:
            self._buildings = [_CanvasBuilding()]
            self._active = 0
        self._active = max(0, min(self._active, len(self._buildings) - 1))
        return self._buildings[self._active]

    @property
    def _vertices(self) -> list:
        return self._b.vertices

    @_vertices.setter
    def _vertices(self, value: list) -> None:
        self._b.vertices = value

    @property
    def _edge_types(self) -> list:
        return self._b.edge_types

    @_edge_types.setter
    def _edge_types(self, value: list) -> None:
        self._b.edge_types = value

    @property
    def _closed(self) -> bool:
        return self._b.closed

    @_closed.setter
    def _closed(self, value: bool) -> None:
        self._b.closed = value

    @property
    def _skeleton_data(self) -> Any:
        return self._b.skeleton_data

    @_skeleton_data.setter
    def _skeleton_data(self, value: Any) -> None:
        self._b.skeleton_data = value

    @property
    def _overhang(self) -> float:
        return self._b.overhang

    @_overhang.setter
    def _overhang(self, value: float) -> None:
        self._b.overhang = value

    # ------------------------------------------------------------------
    # Multi-building API
    # ------------------------------------------------------------------

    def building_count(self) -> int:
        return len(self._buildings)

    def active_index(self) -> int:
        return self._active

    def new_building(self) -> int:
        """Append a fresh empty building, make it active, and return its index."""
        self._buildings.append(_CanvasBuilding())
        self._active = len(self._buildings) - 1
        self.update()
        self.active_changed.emit(self._active)
        return self._active

    def set_active(self, index: int) -> None:
        """Select building *index* as the active one."""
        if 0 <= index < len(self._buildings):
            self._active = index
            self.update()
            self.active_changed.emit(self._active)

    def delete_building(self, index: int) -> None:
        """Remove building *index*; always keep at least one (empty) building."""
        if not (0 <= index < len(self._buildings)):
            return
        del self._buildings[index]
        if not self._buildings:
            self._buildings = [_CanvasBuilding()]
        self._active = min(self._active, len(self._buildings) - 1)
        self.update()

    def set_building_skeleton(self, index: int, data: Any) -> None:
        """Store a RoofResult (or None) for building *index*; schedules repaint."""
        if 0 <= index < len(self._buildings):
            self._buildings[index].skeleton_data = data
            self.update()

    def building_footprint(self, index: int):
        """Return the Footprint of building *index*, or None."""
        if not (0 <= index < len(self._buildings)):
            return None
        b = self._buildings[index]
        if not b.closed or len(b.vertices) < 3:
            return None
        from tectora.core.footprint import Footprint
        try:
            return Footprint.from_lists(b.vertices, b.edge_types)
        except ValueError:
            return None

    def select_building_at(self, px: float, py: float) -> Optional[int]:
        """If (px, py) is inside a closed building's polygon, make it active.

        Returns the selected index, or None if no building was hit. Topmost
        (last-drawn) building wins on overlap.
        """
        for i in range(len(self._buildings) - 1, -1, -1):
            b = self._buildings[i]
            if not b.closed or len(b.vertices) < 3:
                continue
            pts = [self._to_screen(*v) for v in b.vertices]
            if _point_in_polygon(px, py, pts):
                self._active = i
                self.update()
                self.active_changed.emit(self._active)
                return i
        return None

    # ------------------------------------------------------------------
    # Active-building roof parameters (read/write self._b)
    # ------------------------------------------------------------------

    def active_pitch(self) -> int:
        return self._b.pitch

    def set_active_pitch(self, value: int) -> None:
        self._b.pitch = int(value)

    def active_wall_height(self) -> float:
        return self._b.wall_height

    def set_active_wall_height(self, value: float) -> None:
        self._b.wall_height = float(value)

    def active_name(self) -> str:
        return self._b.name

    def set_active_name(self, value: str) -> None:
        self._b.name = str(value)

    # ------------------------------------------------------------------
    # Model API — called by mouse handlers AND by tests
    # ------------------------------------------------------------------

    def add_point(self, mx: float, my: float) -> None:
        """Append a vertex in model space (metres). Only valid while open."""
        if self._closed:
            return
        self._skeleton_data = None
        self._vertices.append((float(mx), float(my)))
        self.update()

    def close_polygon(self) -> None:
        """Finalise the polygon.  Requires ≥ 3 existing vertices and no self-intersection."""
        if len(self._vertices) < 3:
            raise ValueError(
                f"Polygon requires at least 3 vertices; got {len(self._vertices)}."
            )
        from tectora.core.footprint import is_simple_polygon
        if not is_simple_polygon(self._vertices):
            # Stay open so the user can keep editing.
            raise ValueError("Polygon self-intersects; cannot close.")
        self._closed = True
        self._edge_types = ["eave"] * len(self._vertices)
        self.update()

    def move_vertex(self, i: int, mx: float, my: float) -> None:
        """Reposition vertex *i* to model coordinates (mx, my)."""
        self._skeleton_data = None
        self._vertices[i] = (float(mx), float(my))
        self.update()
        if self._closed:
            self.edited.emit()

    def toggle_edge_type(self, i: int) -> None:
        """Flip edge *i* between "eave" and "gable"."""
        if not self._edge_types:
            return
        self._skeleton_data = None
        self._edge_types[i] = "gable" if self._edge_types[i] == "eave" else "eave"
        self.update()
        if self._closed:
            self.edited.emit()

    def clear(self) -> None:
        """Reset to a single empty building (clears ALL buildings)."""
        self._buildings = [_CanvasBuilding()]
        self._active = 0
        self._drag_idx = None
        self._snap_pt = None
        self._pan = (0.0, 0.0)
        self._pan_anchor = None
        self.update()

    @property
    def footprint(self):
        """Return a :class:`Footprint` when the polygon is closed, else ``None``."""
        if not self._closed or len(self._vertices) < 3:
            return None
        from tectora.core.footprint import Footprint
        try:
            return Footprint.from_lists(self._vertices, self._edge_types)
        except ValueError:
            return None

    def set_skeleton(self, data: Any) -> None:
        """Store a RoofResult (or None) for skeleton rendering; schedules repaint."""
        self._skeleton_data = data
        self.update()

    def set_overhang(self, overhang: float) -> None:
        """Store the eave overhang (metres) and schedule a repaint."""
        self._overhang = float(overhang)
        self.update()

    # ------------------------------------------------------------------
    # Zoom API
    # ------------------------------------------------------------------

    _SCALE_MIN: float = 10.0
    _SCALE_MAX: float = 400.0
    _ZOOM_FACTOR: float = 1.2

    def zoom_in(self) -> None:
        """Zoom in by *_ZOOM_FACTOR*, clamped to _SCALE_MAX."""
        self._scale = min(self._SCALE_MAX, self._scale * self._ZOOM_FACTOR)
        self.update()

    def zoom_out(self) -> None:
        """Zoom out by *_ZOOM_FACTOR*, clamped to _SCALE_MIN."""
        self._scale = max(self._SCALE_MIN, self._scale / self._ZOOM_FACTOR)
        self.update()

    def wheelEvent(self, event) -> None:  # type: ignore[override]
        """Zoom canvas with the mouse wheel (centred on widget centre)."""
        delta = event.angleDelta().y()
        if delta == 0:
            super().wheelEvent(event)
            return
        steps = delta / 120.0
        factor = self._ZOOM_FACTOR ** steps
        new_scale = self._scale * factor
        self._scale = max(self._SCALE_MIN, min(self._SCALE_MAX, new_scale))
        self.update()
        event.accept()

    # ------------------------------------------------------------------
    # Coordinate helpers (depend on current widget size)
    # ------------------------------------------------------------------

    def _origin_px(self) -> tuple[float, float]:
        """Pixel position of model (0, 0) — widget centre plus the pan offset."""
        px, py = self._pan
        return self.width() / 2.0 + px, self.height() / 2.0 + py

    def _to_screen(self, mx: float, my: float) -> tuple[float, float]:
        return model_to_screen(mx, my, self._origin_px(), self._scale)

    def _to_model(self, px: float, py: float) -> tuple[float, float]:
        return screen_to_model(px, py, self._origin_px(), self._scale)

    def _snapped_model(self, px: float, py: float) -> tuple[float, float]:
        mx, my = self._to_model(px, py)
        return snap_to_grid(mx, my, _GRID_STEP_M)

    # ------------------------------------------------------------------
    # Mouse event handlers (thin wrappers; logic is in model API above)
    # ------------------------------------------------------------------

    def mousePressEvent(self, event: QMouseEvent) -> None:  # type: ignore[override]
        tool = getattr(self._editor, "active_tool", "draw")
        px, py = float(event.position().x()), float(event.position().y())

        if event.button() == Qt.MouseButton.LeftButton:
            if tool == "draw":
                self._handle_draw_click(px, py)
            elif tool == "editPoints":
                self._handle_editpoints_press(px, py)
            elif tool == "eaveGable":
                self._handle_eavegable_click(px, py)

    def mouseMoveEvent(self, event: QMouseEvent) -> None:  # type: ignore[override]
        px, py = float(event.position().x()), float(event.position().y())
        self._cursor_screen = (px, py)

        # Right-button pan: drag moves the whole view.
        if self._pan_anchor is not None:
            ax, ay, pan0x, pan0y = self._pan_anchor
            self._pan = (pan0x + (px - ax), pan0y + (py - ay))
            self.update()
            return

        # Snap indicator
        mx, my = self._snapped_model(px, py)
        self._snap_pt = (mx, my)

        # Update status label
        self._editor.status_label.setText(
            f"Kurzor  X: {mx:.2f} m  Y: {my:.2f} m"
        )

        # Drag in editPoints mode
        tool = getattr(self._editor, "active_tool", "draw")
        if tool == "editPoints" and self._drag_idx is not None:
            self.move_vertex(self._drag_idx, mx, my)
            return

        self.update()

    def mouseReleaseEvent(self, event: QMouseEvent) -> None:  # type: ignore[override]
        if event.button() == Qt.MouseButton.LeftButton:
            self._drag_idx = None
        elif event.button() == Qt.MouseButton.RightButton and self._pan_anchor is not None:
            self._pan_anchor = None
            self.setCursor(Qt.CursorShape.CrossCursor)

    def _handle_draw_click(self, px: float, py: float) -> None:
        mx, my = self._snapped_model(px, py)

        if self._closed:
            return  # polygon already done

        # If ≥ 3 vertices, check if click is near the first node → close polygon
        if len(self._vertices) >= 3:
            sx, sy = self._to_screen(*self._vertices[0])
            if ((px - sx) ** 2 + (py - sy) ** 2) ** 0.5 <= _CLOSE_SNAP_RADIUS:
                try:
                    self.close_polygon()
                except ValueError:
                    self._editor._show_toast(tr("invalidShape", self._lang))
                return

        self.add_point(mx, my)

    def _handle_editpoints_press(self, px: float, py: float) -> None:
        # Clicking inside another building makes it active first, so the edit
        # then applies to it. A miss leaves the active building unchanged.
        self.select_building_at(px, py)
        if not self._vertices:
            return
        node_pts = [self._to_screen(v[0], v[1]) for v in self._vertices]
        idx = hit_test_node(px, py, node_pts, radius=_NODE_RADIUS + 4)
        self._drag_idx = idx

    def _handle_eavegable_click(self, px: float, py: float) -> None:
        # Clicking inside another building makes it active first, so the
        # eave/gable toggle then applies to it.
        self.select_building_at(px, py)
        if not self._closed or len(self._vertices) < 2:
            return
        n = len(self._vertices)
        segs = []
        for i in range(n):
            a = self._to_screen(*self._vertices[i])
            b = self._to_screen(*self._vertices[(i + 1) % n])
            segs.append((a, b))
        idx = hit_test_edge(px, py, segs, tol=_EDGE_TOL)
        if idx is None:
            return

        # Turning eave -> gable: validate first so we never produce a roof the
        # core would reject. Turning gable -> eave is always allowed.
        current = self._edge_types[idx] if idx < len(self._edge_types) else "eave"
        if current == "eave":
            from tectora.core.gable import is_valid_gable_edge_at
            a = self._vertices[idx]
            b = self._vertices[(idx + 1) % n]
            fp = self.footprint
            if fp is None or not is_valid_gable_edge_at(fp, a, b):
                self._editor._show_toast(tr("gableNotHere", self._lang))
                return

        self.toggle_edge_type(idx)

    # ------------------------------------------------------------------
    # Painting
    # ------------------------------------------------------------------

    def paintEvent(self, event) -> None:  # type: ignore[override]
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)

        self._draw_background(painter)
        self._draw_grid(painter)

        # Draw every building's polygon + roof. The per-building draw helpers
        # read the active building's state, so temporarily point _active at each
        # building in turn (cheap, avoids threading a param through every helper).
        saved_active = self._active
        for i in range(len(self._buildings)):
            self._active = i
            self._draw_polygon(painter)
            self._draw_skeleton(painter)
        self._active = saved_active

        # Overlays apply to the active building only.
        self._draw_active_highlight(painter)
        self._draw_in_progress(painter)
        self._draw_nodes(painter)
        self._draw_snap_indicator(painter)
        self._draw_legend(painter)
        self._draw_clear_button(painter)

        painter.end()

    def _draw_active_highlight(self, p: QPainter) -> None:
        """Tint the active building so the user sees which one is selected.

        Only shown when there is more than one building (no point otherwise).
        """
        if self.building_count() <= 1:
            return
        b = self._b
        if not b.closed or len(b.vertices) < 3:
            return
        from PySide6.QtGui import QPolygonF
        pts = [QPointF(*self._to_screen(*v)) for v in b.vertices]
        fill = QColor(theme.ACCENT)
        fill.setAlpha(28)
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(fill)
        p.drawPolygon(QPolygonF(pts))

    # ── Background ───────────────────────────────────────────────────────

    def _draw_background(self, p: QPainter) -> None:
        p.fillRect(0, 0, self.width(), self.height(), QColor(theme.BG))

    # ── Grid ─────────────────────────────────────────────────────────────

    def _draw_grid(self, p: QPainter) -> None:
        w, h = self.width(), self.height()
        ox, oy = self._origin_px()
        scale = self._scale

        # Compute model extents visible
        mx_min, _ = self._to_model(0, h)
        mx_max, _ = self._to_model(w, 0)
        _, my_min = self._to_model(0, h)
        _, my_max = self._to_model(0, 0)

        step = _GRID_STEP_M

        # Vertical lines
        x_start = int(mx_min / step) - 1
        x_end = int(mx_max / step) + 2
        for xi in range(x_start, x_end):
            mx_val = xi * step
            px_val, _ = self._to_screen(mx_val, 0)
            pen = self._grid_pen(mx_val)
            p.setPen(pen)
            p.drawLine(int(px_val), 0, int(px_val), h)

        # Horizontal lines
        y_start = int(my_min / step) - 1
        y_end = int(my_max / step) + 2
        for yi in range(y_start, y_end):
            my_val = yi * step
            _, py_val = self._to_screen(0, my_val)
            pen = self._grid_pen(my_val)
            p.setPen(pen)
            p.drawLine(0, int(py_val), w, int(py_val))

    def _grid_pen(self, coord: float) -> QPen:
        """Return a QPen with appropriate weight/alpha for the grid line at *coord*."""
        eps = 1e-9
        c = abs(coord)
        if abs(c % _GRID_EMPHASIS_5M) < eps:
            # 5 m emphasis — darker + wider
            pen = QPen(QColor("#d0d3d8"), 1.2)
        elif abs(c % _GRID_EMPHASIS_1M) < eps:
            # 1 m emphasis — slightly darker
            pen = QPen(QColor("#dde0e5"), 0.8)
        else:
            # Minor line (0.5 m)
            pen = QPen(QColor(BORDER_SOFT), 0.6)
        pen.setCosmetic(True)
        return pen

    # ── Polygon ──────────────────────────────────────────────────────────

    def _draw_polygon(self, p: QPainter) -> None:
        if not self._closed or len(self._vertices) < 2:
            return
        # Safety net: a closed polygon that fails to build a Footprint
        # (e.g. self-intersecting) must not render garbage.
        if self.footprint is None:
            return
        n = len(self._vertices)

        has_overhang = self._overhang > 1e-9

        if has_overhang:
            # Draw walls (drawn polygon) muted/thin to distinguish from eave outline
            for i in range(n):
                a = self._vertices[i]
                b = self._vertices[(i + 1) % n]
                pen = QPen(QColor(MUTED), 1.2, Qt.PenStyle.DashLine)
                pen.setCapStyle(Qt.PenCapStyle.RoundCap)
                p.setPen(pen)
                ax, ay = self._to_screen(*a)
                bx, by = self._to_screen(*b)
                p.drawLine(int(ax), int(ay), int(bx), int(by))

            # Draw the offset eave outline in eave-blue
            from tectora.core.poly_offset import offset_polygon
            try:
                eave_verts = offset_polygon(self._vertices, self._overhang)
                eave_color = COL.get("eave", "#4a90d9")
                pen = QPen(QColor(eave_color), 2.5)
                pen.setCapStyle(Qt.PenCapStyle.RoundCap)
                p.setPen(pen)
                m = len(eave_verts)
                for i in range(m):
                    ax, ay = self._to_screen(*eave_verts[i])
                    bx, by = self._to_screen(*eave_verts[(i + 1) % m])
                    p.drawLine(int(ax), int(ay), int(bx), int(by))
            except Exception:
                pass  # If offset fails, skip silently
        else:
            # No overhang: draw footprint edges in their eave/gable colours (current behaviour)
            for i in range(n):
                a = self._vertices[i]
                b = self._vertices[(i + 1) % n]
                etype = self._edge_types[i] if i < len(self._edge_types) else "eave"
                color = COL.get(etype, COL["eave"])
                pen = QPen(QColor(color), 2.5)
                pen.setCapStyle(Qt.PenCapStyle.RoundCap)
                p.setPen(pen)
                ax, ay = self._to_screen(*a)
                bx, by = self._to_screen(*b)
                p.drawLine(int(ax), int(ay), int(bx), int(by))

    # ── Skeleton (ridge / hip / valley) ──────────────────────────────────

    def _draw_skeleton(self, p: QPainter) -> None:
        if self._skeleton_data is None:
            return

        result = self._skeleton_data
        verts = self._vertices
        if not verts:
            return

        # ── Branch: general polygon (SS path) vs rectangle (exact path) ──
        if result.skeleton_segments:
            # SS path: segments are already in model coords (x, y, z)
            import math
            from tectora.gui.editor.skeleton_render import fmt_len
            for kind, a, b in result.skeleton_segments:
                ax, ay = a[0], a[1]
                bx, by = b[0], b[1]
                sx1, sy1 = self._to_screen(ax, ay)
                sx2, sy2 = self._to_screen(bx, by)
                color = COL.get(kind, COL["hip"])
                pen = QPen(QColor(color), 3.0)
                pen.setCapStyle(Qt.PenCapStyle.RoundCap)
                p.setPen(pen)
                p.drawLine(int(sx1), int(sy1), int(sx2), int(sy2))
                # Label midpoint with 3D length
                length3d = math.sqrt(
                    (a[0] - b[0]) ** 2 + (a[1] - b[1]) ** 2 + (a[2] - b[2]) ** 2
                )
                mid_sx = (sx1 + sx2) / 2.0
                mid_sy = (sy1 + sy2) / 2.0
                dx, dy = sx2 - sx1, sy2 - sy1
                seg_len = (dx * dx + dy * dy) ** 0.5
                if seg_len > 1e-3:
                    nx, ny = -dy / seg_len, dx / seg_len
                    mid_sx += nx * 10
                    mid_sy += ny * 10
                label = fmt_len(length3d)
                self._draw_label_at(p, mid_sx, mid_sy, label, color, bold=True)
            # Draw footprint edge-length labels (both paths share this)
            self._draw_edge_length_labels(p)
            self._draw_ss_face_area_labels(p)
        else:
            # Exact path (rectangle): use classify_skeleton + map_calc_point
            from tectora.gui.editor.skeleton_render import (
                classify_skeleton, face_area_labels, fmt_len, fmt_area, map_calc_point
            )
            xs = [v[0] for v in verts]
            ys = [v[1] for v in verts]
            bbox = (min(xs), min(ys), max(xs), max(ys))

            segments = classify_skeleton(result)
            for kind, (cx1, cy1), (cx2, cy2), length3d in segments:
                mx1, my1 = map_calc_point(cx1, cy1, bbox)
                mx2, my2 = map_calc_point(cx2, cy2, bbox)
                sx1, sy1 = self._to_screen(mx1, my1)
                sx2, sy2 = self._to_screen(mx2, my2)
                color = COL.get(kind, COL["hip"])
                pen = QPen(QColor(color), 3.0)
                pen.setCapStyle(Qt.PenCapStyle.RoundCap)
                p.setPen(pen)
                p.drawLine(int(sx1), int(sy1), int(sx2), int(sy2))

            self._draw_edge_length_labels(p)
            self._draw_skeleton_length_labels(p, segments, bbox)
            self._draw_face_area_labels(p, bbox)

    def _dim_font(self) -> QFont:
        """Small annotation font for dimension labels."""
        f = QFont()
        f.setPointSize(7)
        f.setWeight(QFont.Weight.Normal)
        return f

    def _draw_label_at(
        self,
        p: QPainter,
        sx: float,
        sy: float,
        text: str,
        color: str,
        bold: bool = False,
    ) -> None:
        """Draw *text* centred at screen position (sx, sy), clipped to widget."""
        w, h = self.width(), self.height()
        font = self._dim_font()
        if bold:
            font.setWeight(QFont.Weight.DemiBold)
        p.setFont(font)
        p.setPen(QColor(color))
        fm = p.fontMetrics()
        tw = fm.horizontalAdvance(text)
        th = fm.height()
        tx = int(sx - tw / 2)
        ty = int(sy - th / 2)
        # Keep inside widget
        tx = max(2, min(tx, w - tw - 2))
        ty = max(2 + th, min(ty, h - 4))
        p.drawText(tx, ty, text)

    def _draw_edge_length_labels(self, p: QPainter) -> None:
        """Draw edge-length annotations near each footprint edge midpoint."""
        from tectora.gui.editor.skeleton_render import fmt_len
        fp = self.footprint
        if fp is None:
            return
        n = fp.n
        verts = fp.vertices  # 2-D model coords (already in model space)
        for i in range(n):
            ax, ay = verts[i]
            bx, by = verts[(i + 1) % n]
            # Midpoint in model space
            mx = (ax + bx) / 2.0
            my = (ay + by) / 2.0
            # Normal vector (perpendicular to edge, pointing outward ~10 px)
            dx, dy = bx - ax, by - ay
            length = (dx * dx + dy * dy) ** 0.5
            if length < 1e-9:
                continue
            # Outward normal (rotate 90° and offset slightly)
            nx, ny = -dy / length, dx / length
            offset_m = 8.0 / self._scale  # 8 px in model space
            lx, ly = self._to_screen(mx + nx * offset_m, my + ny * offset_m)
            label = fmt_len(fp.edge_length(i))
            self._draw_label_at(p, lx, ly, label, TEXT)

    def _draw_skeleton_length_labels(
        self,
        p: QPainter,
        segments: list,
        bbox: tuple,
    ) -> None:
        """Draw length annotations near each hip/ridge segment midpoint."""
        from tectora.gui.editor.skeleton_render import fmt_len, map_calc_point
        for kind, (cx1, cy1), (cx2, cy2), length3d in segments:
            mx1, my1 = map_calc_point(cx1, cy1, bbox)
            mx2, my2 = map_calc_point(cx2, cy2, bbox)
            # Midpoint in screen space
            sx1, sy1 = self._to_screen(mx1, my1)
            sx2, sy2 = self._to_screen(mx2, my2)
            mid_sx = (sx1 + sx2) / 2.0
            mid_sy = (sy1 + sy2) / 2.0
            # Offset slightly perpendicular to segment
            dx, dy = sx2 - sx1, sy2 - sy1
            seg_len = (dx * dx + dy * dy) ** 0.5
            if seg_len > 1e-3:
                nx, ny = -dy / seg_len, dx / seg_len
                mid_sx += nx * 10
                mid_sy += ny * 10
            color = COL.get(kind, COL["hip"])
            label = fmt_len(length3d)
            self._draw_label_at(p, mid_sx, mid_sy, label, color, bold=True)

    def _draw_face_area_labels(self, p: QPainter, bbox: tuple) -> None:
        """Draw area annotations at each face centroid."""
        from tectora.gui.editor.skeleton_render import face_area_labels, fmt_area, map_calc_point
        labels = face_area_labels(self._skeleton_data)
        for cx, cy, area_m2 in labels:
            mx, my = map_calc_point(cx, cy, bbox)
            sx, sy = self._to_screen(mx, my)
            label = fmt_area(area_m2)
            self._draw_label_at(p, sx, sy, label, MUTED)

    def _draw_ss_face_area_labels(self, p: QPainter) -> None:
        """Per-plane area labels for the SS path (faces are in model coords)."""
        from tectora.gui.editor.skeleton_render import fmt_area
        result = self._skeleton_data
        for face in getattr(result, "faces", None) or []:
            vs = face.vertices
            if not vs:
                continue
            cx = sum(v[0] for v in vs) / len(vs)
            cy = sum(v[1] for v in vs) / len(vs)
            sx, sy = self._to_screen(cx, cy)
            self._draw_label_at(p, sx, sy, fmt_area(face.area_m2), MUTED)

    # ── In-progress polyline ──────────────────────────────────────────────

    def _draw_seg_len_label(
        self,
        p: QPainter,
        a_model: tuple[float, float],
        b_model: tuple[float, float],
        color: str,
    ) -> None:
        """Label the metric length of a model-space segment near its midpoint."""
        from tectora.gui.editor.skeleton_render import fmt_len
        (ax, ay), (bx, by) = a_model, b_model
        length = ((bx - ax) ** 2 + (by - ay) ** 2) ** 0.5
        if length < 1e-6:
            return
        # Midpoint, offset a few px perpendicular to the segment so the label
        # doesn't sit on the line.
        mx, my = (ax + bx) / 2.0, (ay + by) / 2.0
        nx, ny = -(by - ay) / length, (bx - ax) / length
        offset_m = 9.0 / self._scale
        lx, ly = self._to_screen(mx + nx * offset_m, my + ny * offset_m)
        self._draw_label_at(p, lx, ly, fmt_len(length), color, bold=True)

    def _draw_in_progress(self, p: QPainter) -> None:
        if self._closed or len(self._vertices) == 0:
            return
        # Draw existing open segments
        pen = QPen(QColor(COL["eave"]), 2.0)
        pen.setCapStyle(Qt.PenCapStyle.RoundCap)
        p.setPen(pen)
        for i in range(len(self._vertices) - 1):
            ax, ay = self._to_screen(*self._vertices[i])
            bx, by = self._to_screen(*self._vertices[i + 1])
            p.drawLine(int(ax), int(ay), int(bx), int(by))
            # Live length label so the user sees each segment length while
            # drawing, without running the calculation.
            self._draw_seg_len_label(p, self._vertices[i], self._vertices[i + 1], TEXT)

        # Dashed preview segment from last vertex to snapped cursor
        if self._snap_pt is not None and len(self._vertices) >= 1:
            lx, ly = self._to_screen(*self._vertices[-1])
            cx, cy = self._to_screen(*self._snap_pt)
            dash_pen = QPen(QColor(COL["eave"]), 1.5, Qt.PenStyle.DashLine)
            p.setPen(dash_pen)
            p.drawLine(int(lx), int(ly), int(cx), int(cy))
            # Live length of the segment currently being drawn.
            self._draw_seg_len_label(p, self._vertices[-1], self._snap_pt, MUTED)

        # Closing snap highlight: circle around first vertex when close enough
        if len(self._vertices) >= 3 and self._cursor_screen is not None:
            fx, fy = self._to_screen(*self._vertices[0])
            cpx, cpy = self._cursor_screen
            if ((cpx - fx) ** 2 + (cpy - fy) ** 2) ** 0.5 <= _CLOSE_SNAP_RADIUS:
                p.setPen(QPen(QColor(ACCENT), 2.0))
                p.setBrush(Qt.BrushStyle.NoBrush)
                p.drawEllipse(
                    int(fx - _CLOSE_SNAP_RADIUS),
                    int(fy - _CLOSE_SNAP_RADIUS),
                    _CLOSE_SNAP_RADIUS * 2,
                    _CLOSE_SNAP_RADIUS * 2,
                )

    # ── Nodes ─────────────────────────────────────────────────────────────

    def _draw_nodes(self, p: QPainter) -> None:
        if not self._vertices:
            return
        tool = getattr(self._editor, "active_tool", "draw")
        # Draw nodes for closed polygon or while drawing (all placed points)
        for idx, v in enumerate(self._vertices):
            sx, sy = self._to_screen(*v)
            if tool == "editPoints":
                p.setPen(QPen(QColor(ACCENT), 2.0))
                p.setBrush(QColor(CARD))
                p.drawEllipse(
                    int(sx - _NODE_RADIUS), int(sy - _NODE_RADIUS),
                    _NODE_RADIUS * 2, _NODE_RADIUS * 2,
                )
            elif not self._closed:
                # Show small dots for in-progress polyline
                p.setPen(QPen(QColor(ACCENT), 1.5))
                p.setBrush(QColor(CARD))
                r = 4
                p.drawEllipse(int(sx - r), int(sy - r), r * 2, r * 2)

    # ── Snap indicator ────────────────────────────────────────────────────

    def _draw_snap_indicator(self, p: QPainter) -> None:
        tool = getattr(self._editor, "active_tool", "draw")
        if tool != "draw" or self._snap_pt is None or self._closed:
            return
        sx, sy = self._to_screen(*self._snap_pt)
        pen = QPen(QColor(MID), 1.0)
        p.setPen(pen)
        p.setBrush(Qt.BrushStyle.NoBrush)
        r = _SNAP_INDICATOR_R
        p.drawEllipse(int(sx - r), int(sy - r), r * 2, r * 2)

    # ── Legend ────────────────────────────────────────────────────────────

    def _draw_legend(self, p: QPainter) -> None:
        lang = self._lang
        pad = _LEGEND_PAD
        row_h = _LEGEND_ROW_H
        swatch_w = 18
        text_x_off = swatch_w + 8
        total_h = pad * 2 + 14 + 6 + row_h * len(_LEGEND_ITEMS)
        total_w = _LEGEND_W

        # Background box — uses theme.CARD fill and a visible theme.ACCENT border
        p.setPen(QPen(QColor(theme.ACCENT), 1.5))
        p.setBrush(QColor(theme.CARD))
        p.drawRoundedRect(pad, pad, total_w, total_h, 6, 6)

        # Title
        font = QFont()
        font.setPointSize(8)
        font.setWeight(QFont.Weight.DemiBold)
        p.setFont(font)
        p.setPen(QColor(theme.MID))
        p.drawText(pad + 8, pad + 14, tr("legend", lang))

        # Items
        font.setWeight(QFont.Weight.Normal)
        p.setFont(font)
        for row, (key, col_key) in enumerate(_LEGEND_ITEMS):
            y = pad + 14 + 8 + row * row_h
            # Colour swatch
            p.setPen(Qt.PenStyle.NoPen)
            p.setBrush(QColor(COL[col_key]))
            p.drawRect(pad + 8, y + 2, swatch_w, row_h - 6)
            # Label
            p.setPen(QColor(theme.TEXT))
            p.drawText(pad + 8 + text_x_off, y + row_h - 5, tr(key, lang))

    # ── Clear button (top-right) ──────────────────────────────────────────

    def _draw_clear_button(self, p: QPainter) -> None:
        lang = self._lang
        label = tr("clear", lang)
        btn_w, btn_h = 80, 28
        margin = 10
        bx = self.width() - btn_w - margin
        by = margin

        # Store rect so we can hit-test it in mousePressEvent
        self._clear_btn_rect = QRect(bx, by, btn_w, btn_h)

        p.setPen(QPen(QColor(theme.ACCENT), 1.5))
        p.setBrush(QColor(theme.CARD))
        p.drawRoundedRect(bx, by, btn_w, btn_h, 6, 6)

        font = QFont()
        font.setPointSize(9)
        font.setWeight(QFont.Weight.Medium)
        p.setFont(font)
        p.setPen(QColor(theme.MID))
        p.drawText(QRect(bx, by, btn_w, btn_h), Qt.AlignmentFlag.AlignCenter, label)

    # Override mousePressEvent to also handle clear button
    def mousePressEvent(self, event: QMouseEvent) -> None:  # type: ignore[override]
        px, py = float(event.position().x()), float(event.position().y())

        # Right button starts panning the view (works in every tool).
        if event.button() == Qt.MouseButton.RightButton:
            self._pan_anchor = (px, py, self._pan[0], self._pan[1])
            self.setCursor(Qt.CursorShape.ClosedHandCursor)
            return

        # Check clear button first
        if event.button() == Qt.MouseButton.LeftButton:
            if hasattr(self, "_clear_btn_rect") and self._clear_btn_rect.contains(
                int(px), int(py)
            ):
                self.clear()
                return

        tool = getattr(self._editor, "active_tool", "draw")
        if event.button() == Qt.MouseButton.LeftButton:
            if tool == "draw":
                self._handle_draw_click(px, py)
            elif tool == "editPoints":
                self._handle_editpoints_press(px, py)
            elif tool == "eaveGable":
                self._handle_eavegable_click(px, py)
