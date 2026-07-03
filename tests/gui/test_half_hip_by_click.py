"""Polovalba by clicking the end triangle: set its pitch, get the clip depth.

Click a "valba"/"valbička" end face with the Sklon plochy tool and type a
pitch — the roof re-clips so the end face has exactly that pitch
(d = H / tan(pitch)). Steeper end = shallower clip; the exact model is
symmetric so both ends match.
"""
import math
import os
os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

import pytest

from tectora.gui.i18n import tr


def _editor(tmp_path):
    from PySide6.QtWidgets import QApplication
    from tectora.application.project import Project
    from tectora.gui.app_window import MainWindow
    from tectora.io.sqlite_library import SqliteProjectLibrary
    app = QApplication.instance() or QApplication([])
    lib = SqliteProjectLibrary(str(tmp_path / "lib.db"))
    w = MainWindow(library=lib)
    w.open_editor(Project(project_name="Test", global_pitch_deg=35.0))
    e = w._stack.widget(1)
    e.canvas.resize(700, 460)
    return app, w, e


RECT = [(2, 2), (12, 2), (12, 8), (2, 8)]        # 10 × 6, off-origin


def _draw(c, pts=RECT):
    for (x, y) in pts:
        c.add_point(x, y)
    c.close_polygon()


def _apply_end_pitch(c, face_name, value):
    c._open_pitch_editor(face_name, 35.0, 100, 100)
    c._pitch_editor.setText(str(value))
    c._apply_pitch_editor(face_name)


def _capture_toasts(e):
    toasts = []
    e._show_toast = lambda text: toasts.append(text)
    return toasts


# ---------------------------------------------------------------------------
# Core math
# ---------------------------------------------------------------------------

def test_depth_formula_matches_calculator_roundtrip():
    from tectora.core.footprint import Footprint
    from tectora.core.roof_generator import generate_roof, half_hip_depth_for_pitch

    d = half_hip_depth_for_pitch(6.0, 35.0, 60.0)
    assert abs(d - 3 * math.tan(math.radians(35)) / math.tan(math.radians(60))) < 1e-9

    fp = Footprint.from_lists([(0, 0), (10, 0), (10, 6), (0, 6)],
                              ["eave", "gable", "eave", "gable"])
    result = generate_roof(fp, 35.0, half_hip_m=d)
    end = next(f for f in result.faces if "valbička" in f.name)
    assert abs(end.slope_deg - 60.0) < 0.01       # the end face has the asked pitch


def test_depth_formula_rejects_shallow_end():
    from tectora.core.roof_generator import half_hip_depth_for_pitch
    with pytest.raises(ValueError):
        half_hip_depth_for_pitch(6.0, 35.0, 35.0)
    with pytest.raises(ValueError):
        half_hip_depth_for_pitch(6.0, 35.0, 20.0)


# ---------------------------------------------------------------------------
# GUI flow
# ---------------------------------------------------------------------------

def test_click_valba_creates_half_hip_with_that_pitch(tmp_path):
    app, w, e = _editor(tmp_path)
    c = e.canvas
    _draw(c)
    e._on_calculate()                              # plain hip (all eave)

    _apply_end_pitch(c, "levá valba", 60)
    app.processEvents()

    b = c._buildings[0]
    expected_d = 3 * math.tan(math.radians(35)) / math.tan(math.radians(60))
    assert abs(b.half_hip - expected_d) < 1e-6
    # Cap (short) edges got gable-marked so the HALF_HIP mapping applies.
    caps = [i for i, t in enumerate(b.edge_types) if t == "gable"]
    assert len(caps) == 2 and (caps[1] - caps[0]) == 2
    # Recompute produced a half-hip whose end faces have the asked pitch.
    e._on_calculate()
    ends = [f for f in b.skeleton_data.faces if "valbička" in f.name]
    assert len(ends) == 2
    assert all(abs(f.slope_deg - 60.0) < 0.01 for f in ends)
    # Panel spin re-synced.
    assert abs(e._half_hip_spin.value() - round(expected_d, 1)) < 0.051


def test_click_valbicka_changes_existing_half_hip(tmp_path):
    app, w, e = _editor(tmp_path)
    c = e.canvas
    _draw(c)
    e._on_calculate()
    _apply_end_pitch(c, "pravá valba", 60)
    e._on_calculate()

    _apply_end_pitch(c, "pravá valbička", 45)      # steepness down → deeper clip
    e._on_calculate()
    b = c._buildings[0]
    expected_d = 3 * math.tan(math.radians(35))    # tan(45) == 1
    assert abs(b.half_hip - expected_d) < 1e-6
    ends = [f for f in b.skeleton_data.faces if "valbička" in f.name]
    assert all(abs(f.slope_deg - 45.0) < 0.01 for f in ends)


def test_end_pitch_not_steeper_than_main_is_refused(tmp_path):
    app, w, e = _editor(tmp_path)
    c = e.canvas
    _draw(c)
    e._on_calculate()
    toasts = _capture_toasts(e)
    _apply_end_pitch(c, "levá valba", 30)          # main pitch is 35
    b = c._buildings[0]
    assert b.half_hip == 0.0
    assert all(t == "eave" for t in b.edge_types)
    assert tr("halfHipPitchTooLow", "cs") in toasts


def test_end_pitch_on_l_shape_is_refused(tmp_path):
    app, w, e = _editor(tmp_path)
    c = e.canvas
    _draw(c, [(0, 0), (6, 0), (6, 3), (3, 3), (3, 6), (0, 6)])
    e._on_calculate()                              # SS path
    toasts = _capture_toasts(e)
    c._open_pitch_editor("valba", 35.0, 100, 100)
    c._pitch_editor.setText("60")
    c._apply_pitch_editor("valba")
    assert c._buildings[0].half_hip == 0.0
    assert tr("halfHipOnlyRect", "cs") in toasts


def test_end_pitch_click_is_undoable(tmp_path):
    app, w, e = _editor(tmp_path)
    c = e.canvas
    _draw(c)
    e._on_calculate()
    _apply_end_pitch(c, "levá valba", 55)
    assert c._buildings[0].half_hip > 0
    assert c.undo()
    assert c._buildings[0].half_hip == 0.0
    assert all(t == "eave" for t in c._buildings[0].edge_types)
