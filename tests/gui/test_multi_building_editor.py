# -*- coding: utf-8 -*-
"""P3/P4: editor wiring for multi-building designs.

Covers:
  * "+ Nová stavba" path computes EVERY building; the results panel shows the
    Σ-total area across buildings.
  * Selecting a building syncs the parameter controls (pitch spin) to that
    building's own pitch.
  * Single-building regression: ``last_result`` is the building's RoofResult and
    the area row matches it exactly (unchanged from the pre-multi behaviour).
"""
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
    editor = w._stack.widget(1)
    # Real size so model->screen mapping fits both footprints.
    editor.canvas.resize(800, 600)
    return app, editor


def _draw(canvas, pts):
    for (x, y) in pts:
        canvas.add_point(x, y)
    canvas.close_polygon()


# Two non-overlapping rectangles in distinct x ranges so selection is clean.
_B0 = [(0, 0), (12, 0), (12, 8), (0, 8)]      # building 0: 12 x 8
_B1 = [(20, 0), (26, 0), (26, 4), (20, 4)]    # building 1: 6 x 4


def _area_row_value(editor) -> float:
    """Parse the metres² number out of the 'area' results row label."""
    txt = editor._result_value_labels["area"].text()      # e.g. "139.42 m²"
    return float(txt.split(" m")[0].replace(",", "."))


def test_new_building_computes_all_and_area_is_sum(tmp_path):
    from tectora.application import use_cases as uc
    from tectora.application.project import Project

    app, e = _editor(tmp_path)
    c = e.canvas

    _draw(c, _B0)
    e._on_new_building()                       # the new-building path
    assert c.building_count() == 2
    assert c.active_index() == 1
    assert e.active_tool == "draw"
    _draw(c, _B1)

    e._on_calculate()
    app.processEvents()

    # Both buildings now have a roof.
    assert c._buildings[0].skeleton_data is not None
    assert c._buildings[1].skeleton_data is not None

    # Expected total area = Σ of each building computed independently.
    r0 = uc.calculate_project(Project(footprint=c.building_footprint(0), global_pitch_deg=35.0))
    r1 = uc.calculate_project(Project(footprint=c.building_footprint(1), global_pitch_deg=35.0))
    expected = r0.total_area_m2 + r1.total_area_m2

    assert abs(_area_row_value(e) - expected) < 1e-2
    # last_result tracks the ACTIVE building (building 1).
    assert e.last_result is not None
    assert abs(e.last_result.total_area_m2 - r1.total_area_m2) < 1e-6


def test_selecting_building_syncs_pitch(tmp_path):
    app, e = _editor(tmp_path)
    c = e.canvas

    # Building 0 @ pitch 30.
    _draw(c, _B0)
    e.set_pitch(30)
    e._on_new_building()                       # → building 1 active
    # Building 1 @ pitch 45.
    _draw(c, _B1)
    e.set_pitch(45)

    # Select building 0 by clicking inside it → pitch spin shows 30.
    sx, sy = c._to_screen(6, 4)                 # interior of B0
    assert c.select_building_at(sx, sy) == 0
    app.processEvents()
    assert e._pitch_spin.value() == 30
    assert e.pitch == 30

    # Select building 1 → pitch spin shows 45.
    sx, sy = c._to_screen(23, 2)                # interior of B1
    assert c.select_building_at(sx, sy) == 1
    app.processEvents()
    assert e._pitch_spin.value() == 45
    assert e.pitch == 45


def test_single_building_regression(tmp_path):
    from tectora.application import use_cases as uc
    from tectora.application.project import Project

    app, e = _editor(tmp_path)
    c = e.canvas

    _draw(c, _B0)
    e.set_pitch(40)
    e._on_calculate()
    app.processEvents()

    expected = uc.calculate_project(
        Project(footprint=c.footprint, global_pitch_deg=40.0)
    )
    # last_result is exactly the single building's result.
    assert e.last_result is not None
    assert abs(e.last_result.total_area_m2 - expected.total_area_m2) < 1e-6
    # The area row equals that single building's area (totals == single).
    assert abs(_area_row_value(e) - expected.total_area_m2) < 1e-2
