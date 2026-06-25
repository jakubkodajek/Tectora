"""Asymmetric gable: a different back-slope pitch (per building)."""
import os
os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")


# ── Core ────────────────────────────────────────────────────────────────────

def test_back_pitch_changes_gable_geometry():
    from tectora.core.footprint import Footprint
    from tectora.core.roof_generator import generate_roof
    fp = Footprint.from_lists([(0, 0), (8, 0), (8, 5), (0, 5)],
                              ["eave", "gable", "eave", "gable"])
    sym = generate_roof(fp, 35)
    asym = generate_roof(fp, 35, pitch_back=20)
    assert asym.spec.pitch_back == 20.0
    assert abs(asym.apex_height_m - sym.apex_height_m) > 1e-3


def test_back_pitch_ignored_for_hip_and_equal():
    from tectora.core.footprint import Footprint
    from tectora.core.roof_generator import generate_roof
    hip = Footprint.from_lists([(0, 0), (8, 0), (8, 5), (0, 5)], ["eave"] * 4)
    r = generate_roof(hip, 35, pitch_back=20)   # hip → pitch_back not applicable
    assert r.spec.type.value == "hip"
    gab = Footprint.from_lists([(0, 0), (8, 0), (8, 5), (0, 5)],
                               ["eave", "gable", "eave", "gable"])
    r2 = generate_roof(gab, 35, pitch_back=35)  # equal → no asymmetry
    r3 = generate_roof(gab, 35)
    assert abs(r2.apex_height_m - r3.apex_height_m) < 1e-9


# ── GUI panel + persistence ─────────────────────────────────────────────────

def _editor(tmp_path):
    from PySide6.QtWidgets import QApplication
    from tectora.gui.app_window import MainWindow
    from tectora.io.sqlite_library import SqliteProjectLibrary
    from tectora.application.project import Project
    app = QApplication.instance() or QApplication([])
    lib = SqliteProjectLibrary(str(tmp_path / "lib.db"))
    w = MainWindow(library=lib)
    w.open_editor(Project(global_pitch_deg=35.0))
    return app, w, w._stack.widget(1)


def _gable_rect(ed):
    c = ed.canvas
    for x, y in [(0, 0), (8, 0), (8, 5), (0, 5)]:
        c.add_point(x, y)
    c.close_polygon()
    for ei in (1, 3):                # gable the two short ends
        c.toggle_edge_type(ei)


def test_panel_sets_back_pitch_and_recomputes(tmp_path):
    app, w, ed = _editor(tmp_path)
    _gable_rect(ed)
    ed._on_calculate()
    ed._pitch_back_spin.setValue(20)
    app.processEvents()
    assert ed.canvas._buildings[0].pitch_back == 20.0
    assert ed.last_result.spec.pitch_back == 20.0
    # 0 → symmetric (None)
    ed._pitch_back_spin.setValue(0)
    app.processEvents()
    assert ed.canvas._buildings[0].pitch_back is None


def test_back_pitch_persists_and_survives_undo(tmp_path):
    app, w, ed = _editor(tmp_path)
    _gable_rect(ed)
    ed._on_calculate()
    ed._pitch_back_spin.setValue(22)
    app.processEvents()
    ed.name_edit.setText("Asym")
    ed._on_save()
    w.open_entry(w.library.get(w.library.list()[0].id))
    ed2 = w._stack.widget(1)
    assert ed2.canvas._buildings[0].pitch_back == 22.0
    # undo/redo snapshot carries pitch_back
    ed2.canvas._buildings[0].pitch_back = 40.0
    ed2.canvas._push_undo()
    ed2.canvas._buildings[0].pitch_back = 10.0
    ed2.canvas.undo()
    assert ed2.canvas._buildings[0].pitch_back == 40.0
