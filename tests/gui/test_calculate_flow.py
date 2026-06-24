# -*- coding: utf-8 -*-
"""Integration tests: Provést výpočet → skeleton + results panel (G4)."""
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


def test_calculate_gable_fills_results_and_skeleton(tmp_path):
    app, editor = _editor(tmp_path)
    for (x, y) in [(0, 0), (12, 0), (12, 8), (0, 8)]:
        editor.canvas.add_point(x, y)
    editor.canvas.close_polygon()
    editor.canvas.toggle_edge_type(1)  # gable
    editor.canvas.toggle_edge_type(3)  # gable
    editor._on_calculate()
    app.processEvents()
    assert editor.last_result is not None
    assert abs(editor.last_result.ridge_length_m - 12.0) < 1e-6


def test_calculate_without_footprint_no_crash(tmp_path):
    app, editor = _editor(tmp_path)
    editor._on_calculate()  # no polygon → toast, no exception
    app.processEvents()
    assert editor.last_result is None
