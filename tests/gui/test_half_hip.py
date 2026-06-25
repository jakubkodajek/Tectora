"""Half-hip (polovalba): the panel control clips a gabled rectangle into a
half-hip; it is per-building, renders in 3D from the result faces, and persists."""
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
    ed.canvas.resize(800, 500)
    return app, w, ed


def _gabled_rect(c):
    for x, y in [(0, 0), (10, 0), (10, 6), (0, 6)]:
        c.add_point(x, y)
    c.close_polygon()
    c.toggle_edge_type(1)   # gable the two short ends
    c.toggle_edge_type(3)


def test_half_hip_spin_clips_gable_to_half_hip(tmp_path):
    app, w, ed = _editor(tmp_path)
    _gabled_rect(ed.canvas)
    ed._on_calculate()
    assert ed.last_result.spec.type.value == "gable"
    ed._half_hip_spin.setValue(1.0)
    app.processEvents()
    assert ed.last_result.spec.type.value == "half_hip"
    assert ed.last_result.hip_count == 4
    # 3D shows the half-hip's own faces (4 roof planes).
    roof = [v for v, is_wall in ed.view3d._geom_faces if not is_wall]
    assert len(roof) == 4


def test_half_hip_is_per_building(tmp_path):
    app, w, ed = _editor(tmp_path)
    c = ed.canvas
    _gabled_rect(c)
    ed._half_hip_spin.setValue(1.0)
    ed._on_new_building()
    for x, y in [(20, 0), (28, 0), (28, 5), (20, 5)]:
        c.add_point(x, y)
    c.close_polygon()
    assert ed._half_hip_spin.value() == 0.0           # new building has none
    c.select_building_at(*c._to_screen(5, 3))          # back to building 0
    assert ed._half_hip_spin.value() == 1.0            # its value reloads


def test_half_hip_persists(tmp_path):
    app, w, ed = _editor(tmp_path)
    _gabled_rect(ed.canvas)
    ed._half_hip_spin.setValue(0.8)
    ed.name_edit.setText("HH")
    ed._on_save()
    w.open_entry(w.library.get(w.library.list()[0].id))
    ed2 = w._stack.widget(1)
    assert ed2.canvas._buildings[0].half_hip == 0.8


def test_half_hip_zero_keeps_gable(tmp_path):
    app, w, ed = _editor(tmp_path)
    _gabled_rect(ed.canvas)
    ed._half_hip_spin.setValue(0.0)
    ed._on_calculate()
    assert ed.last_result.spec.type.value == "gable"
