# -*- coding: utf-8 -*-
"""SS6: offscreen tests for general-polygon skeleton rendering."""
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


def test_lshape_calculates_and_has_valley(tmp_path):
    app, editor = _editor(tmp_path)
    for (x, y) in [(0, 0), (6, 0), (6, 3), (3, 3), (3, 5), (0, 5)]:
        editor.canvas.add_point(x, y)
    editor.canvas.close_polygon()
    editor._on_calculate()
    app.processEvents()
    assert editor.last_result is not None
    assert editor.last_result.valley_length_m > 0
    assert any(s[0] == "valley" for s in editor.last_result.skeleton_segments)


def test_rectangle_still_renders_via_faces(tmp_path):
    app, editor = _editor(tmp_path)
    for (x, y) in [(0, 0), (12, 0), (12, 8), (0, 8)]:
        editor.canvas.add_point(x, y)
    editor.canvas.close_polygon()
    editor._on_calculate()
    app.processEvents()
    assert editor.last_result is not None
    assert editor.last_result.hip_count == 4        # rectangle → exact path
    assert len(editor.last_result.faces) == 4
