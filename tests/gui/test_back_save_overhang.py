"""Back-confirm dialog, auto-name on save, save-all-buildings, per-building
overhang, and the view-switch repaint fix."""
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


def _rect(c, pts):
    for x, y in pts:
        c.add_point(x, y)
    c.close_polygon()


def test_save_persists_all_buildings_with_autoname(tmp_path):
    app, w, ed = _editor(tmp_path)
    c = ed.canvas
    _rect(c, [(0, 0), (6, 0), (6, 4), (0, 4)])
    ed._overhang_spin.setValue(0.0)
    ed._on_new_building()
    _rect(c, [(12, 0), (18, 0), (18, 4), (12, 4)])
    ed._overhang_spin.setValue(1.0)

    ed.name_edit.setText("")
    ed._on_save()

    e = w.library.list()[0]
    assert e.name == "Stavba #1"
    assert len(e.project.buildings) == 2
    assert [b.overhang_m for b in e.project.buildings] == [0.0, 1.0]


def test_default_project_name_increments(tmp_path):
    app, w, ed = _editor(tmp_path)
    assert ed._default_project_name() == "Stavba #1"
    _rect(ed.canvas, [(0, 0), (6, 0), (6, 4), (0, 4)])
    ed.name_edit.setText("")
    ed._on_save()
    assert ed._default_project_name() == "Stavba #2"


def test_per_building_overhang_independent(tmp_path):
    app, w, ed = _editor(tmp_path)
    c = ed.canvas
    _rect(c, [(0, 0), (6, 0), (6, 4), (0, 4)])
    ed._overhang_spin.setValue(0.0)
    ed._on_new_building()
    _rect(c, [(12, 0), (18, 0), (18, 4), (12, 4)])
    ed._overhang_spin.setValue(1.0)
    assert c._buildings[0].overhang == 0.0
    assert c._buildings[1].overhang == 1.0
    # Selecting a building reloads the overhang spin from that building.
    sx, sy = c._to_screen(3, 2)
    c.select_building_at(sx, sy)
    assert ed._overhang_spin.value() == 0.0
    sx, sy = c._to_screen(15, 2)
    c.select_building_at(sx, sy)
    assert ed._overhang_spin.value() == 1.0


def test_back_confirm_cancel_stays(tmp_path):
    app, w, ed = _editor(tmp_path)
    _rect(ed.canvas, [(0, 0), (6, 0), (6, 4), (0, 4)])
    calls = {"home": 0}
    w.open_home = lambda: calls.__setitem__("home", calls["home"] + 1)
    ed._confirm_leave = lambda: "cancel"
    ed._on_back()
    assert calls["home"] == 0  # stayed
    assert len(w.library.list()) == 0  # not saved


def test_back_confirm_save_then_home(tmp_path):
    app, w, ed = _editor(tmp_path)
    _rect(ed.canvas, [(0, 0), (6, 0), (6, 4), (0, 4)])
    calls = {"home": 0}
    w.open_home = lambda: calls.__setitem__("home", calls["home"] + 1)
    ed._confirm_leave = lambda: "save"
    ed.name_edit.setText("")
    ed._on_back()
    assert calls["home"] == 1
    assert len(w.library.list()) == 1
    assert w.library.list()[0].name == "Stavba #1"


def test_back_no_drawing_goes_straight_home(tmp_path):
    app, w, ed = _editor(tmp_path)
    calls = {"home": 0, "asked": 0}
    w.open_home = lambda: calls.__setitem__("home", calls["home"] + 1)
    ed._confirm_leave = lambda: (calls.__setitem__("asked", 1), "cancel")[1]
    ed._on_back()
    assert calls["home"] == 1   # went home
    assert calls["asked"] == 0  # never prompted (nothing to save)


def test_view_switch_repaints_without_error(tmp_path):
    app, w, ed = _editor(tmp_path)
    _rect(ed.canvas, [(0, 0), (6, 0), (6, 4), (0, 4)])
    ed._on_calculate()
    ed._set_view_mode("3d")
    ed._set_view_mode("2d")
    assert ed.view_mode == "2d"
    # Data survives the round-trip (the bug was a missing repaint, not data loss).
    assert ed.canvas._closed is True
    assert ed.canvas.footprint is not None
