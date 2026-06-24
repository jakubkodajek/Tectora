# -*- coding: utf-8 -*-
"""Tests for live re-generation on canvas edits (G4b)."""
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


def test_toggle_after_calc_regenerates(tmp_path):
    app, e = _editor(tmp_path)
    for (x, y) in [(0, 0), (12, 0), (12, 8), (0, 8)]:
        e.canvas.add_point(x, y)
    e.canvas.close_polygon()
    e._on_calculate()                      # 4 eaves → hip
    app.processEvents()
    assert e.last_result.hip_count == 4
    e.canvas.toggle_edge_type(1)           # → gable
    e.canvas.toggle_edge_type(3)           # → gable; live regen
    app.processEvents()
    assert e.last_result.hip_count == 0    # now a gable roof, recomputed automatically
    assert abs(e.last_result.ridge_length_m - 12.0) < 1e-6


def test_set_pitch_recomputes(tmp_path):
    app, e = _editor(tmp_path)
    for (x, y) in [(0, 0), (12, 0), (12, 8), (0, 8)]:
        e.canvas.add_point(x, y)
    e.canvas.close_polygon()
    e._on_calculate()
    app.processEvents()
    area_35 = e.last_result.total_area_m2
    e.set_pitch(50)
    app.processEvents()
    assert e.last_result.total_area_m2 > area_35  # steeper → more area
