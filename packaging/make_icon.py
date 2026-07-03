"""Render the Tectora logo (rounded accent square + house glyph) into an .ico.

Usage: python packaging/make_icon.py [output.ico]

Draws the same glyph as the home screen's logo widget, scaled from its 26 px
design grid, so the app icon matches the in-app branding.
"""
from __future__ import annotations

import os
import sys

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

from PySide6.QtCore import QPointF, Qt  # noqa: E402
from PySide6.QtGui import (  # noqa: E402
    QColor,
    QGuiApplication,
    QPainter,
    QPainterPath,
    QPen,
    QPixmap,
)

from tectora.gui import theme  # noqa: E402

# House glyph line segments on the 26 px design grid (see home _LogoWidget).
_GLYPH_LINES = [
    (4, 12, 13, 6),    # roof left
    (13, 6, 22, 12),   # roof right
    (6, 12, 6, 20),    # wall left
    (6, 20, 20, 20),   # floor
    (20, 20, 20, 12),  # wall right
]


def render_logo(size: int) -> QPixmap:
    pm = QPixmap(size, size)
    pm.fill(Qt.GlobalColor.transparent)
    p = QPainter(pm)
    p.setRenderHint(QPainter.RenderHint.Antialiasing)
    s = size / 26.0

    path = QPainterPath()
    path.addRoundedRect(0, 0, size, size, 6 * s, 6 * s)
    p.fillPath(path, QColor(theme.ACCENT))

    pen = QPen(QColor("#ffffff"), 1.7 * s)
    pen.setCapStyle(Qt.PenCapStyle.RoundCap)
    pen.setJoinStyle(Qt.PenJoinStyle.RoundJoin)
    p.setPen(pen)
    p.setBrush(Qt.BrushStyle.NoBrush)
    for (x1, y1, x2, y2) in _GLYPH_LINES:
        p.drawLine(QPointF(x1 * s, y1 * s), QPointF(x2 * s, y2 * s))
    p.end()
    return pm


def main() -> int:
    out = sys.argv[1] if len(sys.argv) > 1 else os.path.join(
        os.path.dirname(__file__), "tectora.ico")
    QGuiApplication([])
    pm = render_logo(256)
    if not pm.save(out, "ICO"):
        print(f"Nepodařilo se zapsat ikonu: {out}", file=sys.stderr)
        return 1
    print(f"Ikona zapsána: {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
