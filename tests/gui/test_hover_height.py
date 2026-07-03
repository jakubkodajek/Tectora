"""Hover-height tooltip: resting on a ridge/eave reports its height."""
import os
os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")


def _editor(tmp_path):
    from PySide6.QtWidgets import QApplication
    from tectora.gui.app_window import MainWindow
    from tectora.io.sqlite_library import SqliteProjectLibrary
    from tectora.application.project import Project
    app = QApplication.instance() or QApplication([])
    lib = SqliteProjectLibrary(str(tmp_path / "lib.db"))
    w = MainWindow(library=lib)
    w.open_editor(Project(global_pitch_deg=35.0))
    ed = w._stack.widget(1)
    ed.canvas.resize(700, 460)
    return app, w, ed


def _paint(c):
    from PySide6.QtGui import QPixmap
    pm = QPixmap(c.size())
    c.render(pm)


def test_ridge_hover_shows_absolute_height(tmp_path):
    import math
    app, w, ed = _editor(tmp_path)
    c = ed.canvas
    for x, y in [(2, 2), (12, 2), (12, 8), (2, 8)]:  # 10 x 6 rectangle
        c.add_point(x, y)
    c.close_polygon()
    ed._on_calculate()
    _paint(c)
    assert len(c._painted_ridges) >= 1
    s = c._painted_ridges[0]
    mx, my = (s[0] + s[2]) / 2, (s[1] + s[3]) / 2
    text = c._hover_text_at(mx, my)
    # Absolute height above ground: eave (wall 2.5 + base 0) + apex.
    # 10x6 hip at 35°: apex = (6/2)*tan(35) ≈ 2.10 m → ridge ≈ 4.60 m.
    expected = 2.5 + (6 / 2) * math.tan(math.radians(35))
    assert text is not None and "m" in text
    assert abs(float(text.split(":")[1].strip().rstrip(" m")) - expected) < 0.05


def test_eave_hover_shows_eave_height(tmp_path):
    app, w, ed = _editor(tmp_path)
    c = ed.canvas
    for x, y in [(2, 2), (12, 2), (12, 8), (2, 8)]:
        c.add_point(x, y)
    c.close_polygon()
    ed._on_calculate()
    _paint(c)
    ex, ey = c._to_screen(7, 2)   # midpoint of the bottom eave edge
    text = c._hover_text_at(ex, ey)
    # Default: base 0 + wall height 2.5 → the eave line is 2.50 m above ground.
    assert text is not None and text.endswith("2.50 m")


def test_hover_miss_returns_none(tmp_path):
    app, w, ed = _editor(tmp_path)
    c = ed.canvas
    for x, y in [(2, 2), (12, 2), (12, 8), (2, 8)]:
        c.add_point(x, y)
    c.close_polygon()
    ed._on_calculate()
    _paint(c)
    # A point well outside the roof/eaves.
    assert c._hover_text_at(5.0, 5.0) is None


def test_hover_on_lshape_ridge(tmp_path):
    app, w, ed = _editor(tmp_path)
    c = ed.canvas
    for x, y in [(0, 0), (6, 0), (6, 3), (3, 3), (3, 6), (0, 6)]:
        c.add_point(x, y)
    c.close_polygon()
    ed._on_calculate()
    _paint(c)
    assert len(c._painted_ridges) >= 1
    s = c._painted_ridges[0]
    mx, my = (s[0] + s[2]) / 2, (s[1] + s[3]) / 2
    assert "m" in (c._hover_text_at(mx, my) or "")
