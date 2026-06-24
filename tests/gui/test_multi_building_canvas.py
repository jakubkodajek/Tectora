"""P3 step 1: Canvas2D holds multiple buildings (active-index delegation)."""
import os
os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

from tectora.gui.editor.canvas_geometry import point_in_polygon


def _canvas():
    from PySide6.QtWidgets import QApplication
    from tectora.gui.app_window import MainWindow
    from tectora.io.sqlite_library import SqliteProjectLibrary
    import tempfile, os as _os
    app = QApplication.instance() or QApplication([])
    lib = SqliteProjectLibrary(_os.path.join(tempfile.mkdtemp(), "lib.db"))
    w = MainWindow(library=lib)
    from tectora.application.project import Project
    w.open_editor(Project(global_pitch_deg=35.0))
    c = w._stack.widget(1).canvas
    c.resize(700, 500)
    return c


def _draw(c, pts):
    for x, y in pts:
        c.add_point(x, y)
    c.close_polygon()


def test_starts_with_one_empty_building():
    c = _canvas()
    assert c.building_count() == 1
    assert c.active_index() == 0
    assert c.footprint is None


def test_new_building_appends_and_activates():
    c = _canvas()
    _draw(c, [(0, 0), (6, 0), (6, 4), (0, 4)])
    assert c.building_count() == 1 and c.footprint is not None
    idx = c.new_building()
    assert idx == 1 and c.active_index() == 1
    assert c.building_count() == 2
    assert c.footprint is None  # the new active building is empty
    _draw(c, [(10, 0), (16, 0), (16, 4), (10, 4)])
    assert c.building_footprint(0) is not None
    assert c.building_footprint(1) is not None


def test_select_building_at_interior_point():
    c = _canvas()
    _draw(c, [(0, 0), (6, 0), (6, 4), (0, 4)])
    c.new_building()
    _draw(c, [(10, 0), (16, 0), (16, 4), (10, 4)])
    assert c.active_index() == 1
    sx, sy = c._to_screen(3, 2)            # inside building 0
    assert c.select_building_at(sx, sy) == 0
    assert c.active_index() == 0
    # A point in empty space hits nothing.
    fx, fy = c._to_screen(50, 50)
    assert c.select_building_at(fx, fy) is None


def test_delete_building_keeps_at_least_one():
    c = _canvas()
    _draw(c, [(0, 0), (6, 0), (6, 4), (0, 4)])
    c.new_building()
    _draw(c, [(10, 0), (16, 0), (16, 4), (10, 4)])
    assert c.building_count() == 2
    c.delete_building(1)
    assert c.building_count() == 1
    c.delete_building(0)
    assert c.building_count() == 1   # never drops below one
    assert c.footprint is None


def test_clear_resets_all_buildings():
    c = _canvas()
    _draw(c, [(0, 0), (6, 0), (6, 4), (0, 4)])
    c.new_building()
    _draw(c, [(10, 0), (16, 0), (16, 4), (10, 4)])
    c.clear()
    assert c.building_count() == 1
    assert c.footprint is None


def test_point_in_polygon_helper():
    sq = [(0, 0), (10, 0), (10, 10), (0, 10)]
    assert point_in_polygon(5, 5, sq) is True
    assert point_in_polygon(15, 5, sq) is False
    assert point_in_polygon(1, 1, [(0, 0), (1, 1)]) is False  # degenerate
