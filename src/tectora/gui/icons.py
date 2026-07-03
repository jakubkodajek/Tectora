# -*- coding: utf-8 -*-
"""Painted toolbar icons.

Unicode glyphs (↶ ↷ ⟳ ← ✎ ⧉ ✕) are not present in every Windows UI font, so
buttons using them can render as empty squares. These icons are drawn with
QPainter instead — always visible, and they follow the current theme colour.
Icons are rendered at 40 px and scaled down by the button's iconSize.
"""
from __future__ import annotations

import math

from PySide6.QtCore import QPointF, QRectF, Qt
from PySide6.QtGui import (
    QColor,
    QIcon,
    QPainter,
    QPainterPath,
    QPen,
    QPixmap,
    QTransform,
)

from tectora.gui import theme

_SIZE = 40.0


def _paint(draw_fn, color: str | None = None, width: float = 3.4) -> QPixmap:
    pm = QPixmap(int(_SIZE), int(_SIZE))
    pm.fill(Qt.GlobalColor.transparent)
    p = QPainter(pm)
    p.setRenderHint(QPainter.RenderHint.Antialiasing)
    pen = QPen(QColor(color or theme.MID), width)
    pen.setCapStyle(Qt.PenCapStyle.RoundCap)
    pen.setJoinStyle(Qt.PenJoinStyle.RoundJoin)
    p.setPen(pen)
    p.setBrush(Qt.BrushStyle.NoBrush)
    draw_fn(p, _SIZE)
    p.end()
    return pm


def _arc_arrow(p: QPainter, s: float, start_deg: float, sweep_deg: float) -> None:
    """Circular arc with an arrowhead at its end (for undo/redo/rotate)."""
    r = QRectF(s * 0.22, s * 0.22, s * 0.56, s * 0.56)
    path = QPainterPath()
    path.arcMoveTo(r, start_deg)
    path.arcTo(r, start_deg, sweep_deg)
    p.drawPath(path)

    cx, cy = r.center().x(), r.center().y()
    rx, ry = r.width() / 2.0, r.height() / 2.0

    def pt(deg: float) -> QPointF:
        a = math.radians(deg)
        return QPointF(cx + rx * math.cos(a), cy - ry * math.sin(a))

    end_deg = start_deg + sweep_deg
    tip = pt(end_deg)
    back = pt(end_deg - math.copysign(10.0, sweep_deg))
    dx, dy = tip.x() - back.x(), tip.y() - back.y()
    ln = math.hypot(dx, dy) or 1.0
    dx, dy = dx / ln, dy / ln
    for ang in (math.radians(150.0), math.radians(-150.0)):
        bx = dx * math.cos(ang) - dy * math.sin(ang)
        by = dx * math.sin(ang) + dy * math.cos(ang)
        p.drawLine(tip, QPointF(tip.x() + bx * s * 0.2, tip.y() + by * s * 0.2))


def undo_icon(color: str | None = None) -> QIcon:
    """Counterclockwise arc arrow (↶)."""
    return QIcon(_paint(lambda p, s: _arc_arrow(p, s, 30, 180), color))


def redo_icon(color: str | None = None) -> QIcon:
    """Clockwise arc arrow (↷) — mirror of undo."""
    pm = _paint(lambda p, s: _arc_arrow(p, s, 30, 180), color)
    return QIcon(pm.transformed(QTransform().scale(-1, 1)))


def rotate_icon(color: str | None = None) -> QIcon:
    """Nearly-full clockwise circle with an arrowhead (⟳)."""
    return QIcon(_paint(lambda p, s: _arc_arrow(p, s, 100, -300), color))


def arrow_icon(direction: str, color: str | None = None) -> QIcon:
    """Straight arrow; *direction* ∈ left / right / up / down."""
    def draw(p: QPainter, s: float) -> None:
        m, c = s * 0.2, s / 2.0
        p.drawLine(QPointF(s - m, c), QPointF(m, c))
        p.drawLine(QPointF(m, c), QPointF(m + s * 0.2, c - s * 0.2))
        p.drawLine(QPointF(m, c), QPointF(m + s * 0.2, c + s * 0.2))

    pm = _paint(draw, color)
    deg = {"left": 0, "up": 90, "right": 180, "down": 270}[direction]
    if deg:
        pm = pm.transformed(QTransform().rotate(deg))
    return QIcon(pm)


def rename_icon(color: str | None = None) -> QIcon:
    """Pencil (✎)."""
    def draw(p: QPainter, s: float) -> None:
        p.drawLine(QPointF(s * 0.28, s * 0.74), QPointF(s * 0.66, s * 0.36))
        p.drawLine(QPointF(s * 0.60, s * 0.28), QPointF(s * 0.74, s * 0.42))
        # nib tick at the writing end
        p.drawLine(QPointF(s * 0.28, s * 0.74), QPointF(s * 0.24, s * 0.78))
    return QIcon(_paint(draw, color, width=3.0))


def duplicate_icon(color: str | None = None) -> QIcon:
    """Two overlapping squares (⧉)."""
    def draw(p: QPainter, s: float) -> None:
        p.drawRect(QRectF(s * 0.34, s * 0.22, s * 0.42, s * 0.42))
        p.drawRect(QRectF(s * 0.22, s * 0.36, s * 0.42, s * 0.42))
    return QIcon(_paint(draw, color, width=2.8))


def close_icon(color: str | None = None) -> QIcon:
    """Cross (✕)."""
    def draw(p: QPainter, s: float) -> None:
        p.drawLine(QPointF(s * 0.3, s * 0.3), QPointF(s * 0.7, s * 0.7))
        p.drawLine(QPointF(s * 0.7, s * 0.3), QPointF(s * 0.3, s * 0.7))
    return QIcon(_paint(draw, color))
