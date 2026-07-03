"""Removing a building from the UI (− Odebrat stavbu / Delete key handler)."""
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
    w.open_editor(Project(project_name="Test", global_pitch_deg=35.0))
    return app, w, w._stack.widget(1)


def _draw(c, pts):
    for (x, y) in pts:
        c.add_point(x, y)
    c.close_polygon()


RECT_A = [(0, 0), (10, 0), (10, 6), (0, 6)]
RECT_B = [(20, 0), (26, 0), (26, 4), (20, 4)]


def test_delete_active_building_updates_totals(tmp_path):
    app, w, e = _editor(tmp_path)
    c = e.canvas
    _draw(c, RECT_A)
    e._on_new_building()
    _draw(c, RECT_B)
    e._on_calculate()
    total_two = e._result_value_labels["area"].text()

    e._on_delete_building()          # removes the active (second) building
    assert c.building_count() == 1
    total_one = e._result_value_labels["area"].text()
    assert total_one != total_two    # totals recomputed without the removed roof


def test_delete_last_drawn_building_clears_results(tmp_path):
    app, w, e = _editor(tmp_path)
    c = e.canvas
    _draw(c, RECT_A)
    e._on_calculate()
    assert e.last_result is not None

    e._on_delete_building()
    assert c.building_count() == 1               # a fresh empty one remains
    assert c.building_footprint(0) is None
    assert e.last_result is None
    assert e._result_value_labels["area"].text() == "—"


def test_delete_on_lone_empty_building_is_noop(tmp_path):
    app, w, e = _editor(tmp_path)
    c = e.canvas
    assert c.building_count() == 1
    before_undo = c.can_undo()
    e._on_delete_building()
    assert c.building_count() == 1
    assert c.can_undo() == before_undo           # nothing was pushed/changed


def test_deleted_building_comes_back_with_undo(tmp_path):
    app, w, e = _editor(tmp_path)
    c = e.canvas
    _draw(c, RECT_A)
    e._on_new_building()
    _draw(c, RECT_B)
    e._on_delete_building()
    assert c.building_count() == 1
    e._on_undo()
    assert c.building_count() == 2
    assert c.building_footprint(1) is not None


def test_remove_button_exists_in_toolbar(tmp_path):
    app, w, e = _editor(tmp_path)
    assert hasattr(e, "_remove_building_btn")
    assert e._remove_building_btn.text() != ""
