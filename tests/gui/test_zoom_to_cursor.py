"""Wheel zoom anchors at the cursor: the model point under it stays put."""
import os
os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")


def _canvas(tmp_path):
    from PySide6.QtWidgets import QApplication
    from tectora.application.project import Project
    from tectora.gui.app_window import MainWindow
    from tectora.io.sqlite_library import SqliteProjectLibrary
    app = QApplication.instance() or QApplication([])
    lib = SqliteProjectLibrary(str(tmp_path / "lib.db"))
    w = MainWindow(library=lib)
    w.open_editor(Project(global_pitch_deg=35.0))
    c = w._stack.widget(1).canvas
    c.resize(800, 600)
    return app, w, c


def test_zoom_at_keeps_cursor_model_point(tmp_path):
    app, w, c = _canvas(tmp_path)
    px, py = 620.0, 130.0          # off-centre cursor position
    before = c._to_model(px, py)
    c.zoom_at(px, py, 1.44)
    after = c._to_model(px, py)
    assert abs(before[0] - after[0]) < 1e-9
    assert abs(before[1] - after[1]) < 1e-9
    c.zoom_at(px, py, 0.5)
    again = c._to_model(px, py)
    assert abs(before[0] - again[0]) < 1e-9
    assert abs(before[1] - again[1]) < 1e-9


def test_zoom_at_respects_scale_clamps(tmp_path):
    app, w, c = _canvas(tmp_path)
    for _ in range(50):
        c.zoom_at(400, 300, 2.0)
    assert c._scale == c._SCALE_MAX
    for _ in range(80):
        c.zoom_at(400, 300, 0.5)
    assert c._scale == c._SCALE_MIN


def test_zoom_at_noop_at_limit_keeps_pan(tmp_path):
    app, w, c = _canvas(tmp_path)
    c._scale = c._SCALE_MAX
    pan_before = c._pan
    c.zoom_at(100, 100, 2.0)       # already at max — nothing may move
    assert c._pan == pan_before
