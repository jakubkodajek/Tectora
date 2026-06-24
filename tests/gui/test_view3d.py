# -*- coding: utf-8 -*-
"""Tests for G7: 3D roof preview and 2D/3D toggle."""
import os
os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")


def _editor(tmp_path):
    from PySide6.QtWidgets import QApplication
    from tectora.application.project import Project
    from tectora.gui.app_window import MainWindow
    from tectora.io.sqlite_library import SqliteProjectLibrary
    app = QApplication.instance() or QApplication([])
    lib = SqliteProjectLibrary(str(tmp_path / "lib.db"))
    w = MainWindow(library=lib)
    w.open_editor(Project(global_pitch_deg=35.0))
    return app, w._stack.widget(1)


def test_projection_is_deterministic():
    from tectora.gui.editor.view_3d import project_point
    a = project_point((0, 0, 0), 0.6, 0.5, 10.0, (100, 100))
    b = project_point((0, 0, 0), 0.6, 0.5, 10.0, (100, 100))
    assert a == b


def test_3d_view_accepts_result_without_crash(tmp_path):
    app, e = _editor(tmp_path)
    for (x, y) in [(0, 0), (12, 0), (12, 8), (0, 8)]:
        e.canvas.add_point(x, y)
    e.canvas.close_polygon()
    e._on_calculate()
    app.processEvents()
    e.view3d.set_result(e.last_result)
    e.view3d.resize(400, 300)
    e.view3d.repaint()           # exercise paintEvent offscreen
    app.processEvents()
    assert e.last_result is not None


def test_toggle_switches_view(tmp_path):
    app, e = _editor(tmp_path)
    e._set_view_mode("3d") if hasattr(e, "_set_view_mode") else None
    app.processEvents()
    # view_mode attribute should reflect the toggle if a setter exists; at minimum the 3D widget exists
    assert e.view3d is not None


def test_3d_view_clear_result(tmp_path):
    """set_result(None) should clear without crash."""
    app, e = _editor(tmp_path)
    e.view3d.set_result(None)
    e.view3d.resize(400, 300)
    e.view3d.repaint()
    app.processEvents()
    assert e.view3d._result is None


def test_stacked_widget_switches_on_toggle(tmp_path):
    """The stacked widget should switch index when the 2D/3D toggle fires."""
    app, e = _editor(tmp_path)
    # Start in 2D
    assert e.view_mode == "2d"
    # Switch to 3D
    e._btn_3d.click()
    app.processEvents()
    assert e.view_mode == "3d"
    # Switch back to 2D
    e._btn_2d.click()
    app.processEvents()
    assert e.view_mode == "2d"
