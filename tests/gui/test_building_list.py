"""STAVBY panel: list of the design's buildings with select + remove."""
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
    return app, w, w._stack.widget(1)


RECT_A = [(0, 0), (10, 0), (10, 6), (0, 6)]
RECT_B = [(20, 0), (26, 0), (26, 4), (20, 4)]


def _draw(c, pts):
    for (x, y) in pts:
        c.add_point(x, y)
    c.close_polygon()


def test_fresh_editor_lists_one_empty_building(tmp_path):
    app, w, e = _editor(tmp_path)
    assert len(e._building_rows) == 1
    label = e._building_rows[0][0].text()
    assert "Stavba 1" in label
    assert "(prázdná)" in label


def test_closing_polygon_drops_empty_suffix(tmp_path):
    app, w, e = _editor(tmp_path)
    _draw(e.canvas, RECT_A)
    label = e._building_rows[0][0].text()
    assert "(prázdná)" not in label


def test_new_building_adds_a_row(tmp_path):
    app, w, e = _editor(tmp_path)
    _draw(e.canvas, RECT_A)
    e._on_new_building()
    assert len(e._building_rows) == 2
    assert e.canvas.active_index() == 1


def test_row_click_selects_building_and_syncs_panel(tmp_path):
    app, w, e = _editor(tmp_path)
    c = e.canvas
    _draw(c, RECT_A)
    e._pitch_spin.setValue(40)
    e._on_new_building()
    _draw(c, RECT_B)
    e._pitch_spin.setValue(25)

    e._building_rows[0][0].click()
    assert c.active_index() == 0
    assert e._pitch_spin.value() == 40


def test_row_delete_removes_that_building(tmp_path):
    app, w, e = _editor(tmp_path)
    c = e.canvas
    _draw(c, RECT_A)
    e._on_new_building()
    _draw(c, RECT_B)
    assert len(e._building_rows) == 2

    e._building_rows[0][3].click()       # ✕ on the first building (last button)
    assert c.building_count() == 1
    assert len(e._building_rows) == 1
    # The remaining building is the former second one.
    assert c.building_footprint(0).vertices[0] == (20.0, 0.0)


def test_load_project_populates_list(tmp_path):
    from tectora.application.project import Building, Project
    from tectora.core.footprint import Footprint
    app, w, e = _editor(tmp_path)
    proj = Project(project_name="Dva", buildings=[
        Building(footprint=Footprint.from_lists(RECT_A, ["eave"] * 4), name="Dům"),
        Building(footprint=Footprint.from_lists(RECT_B, ["eave"] * 4)),
    ])
    e.load_project(proj)
    labels = [row[0].text() for row in e._building_rows]
    assert len(labels) == 2
    assert labels[0] == "Dům"            # named building keeps its name
    assert "Stavba 2" in labels[1]       # unnamed falls back to Stavba N
