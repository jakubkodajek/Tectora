"""Nested buildings: node grabbing and selection must reach the INNER one.

Reported: with a building drawn inside another (tower on a base), the edit
tool always grabbed the larger building — its polygon contains every click
aimed at the small one, so the small one's corners were unreachable.
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
    e = w._stack.widget(1)
    e.canvas.resize(800, 600)
    return app, w, e


BIG = [(0, 0), (12, 0), (12, 8), (0, 8)]
SMALL = [(4, 3), (7, 3), (7, 5), (4, 5)]     # fully inside BIG


def _draw(c, pts):
    for (x, y) in pts:
        c.add_point(x, y)
    c.close_polygon()


def _nested_editor(tmp_path, big_first=True):
    app, w, e = _editor(tmp_path)
    c = e.canvas
    first, second = (BIG, SMALL) if big_first else (SMALL, BIG)
    _draw(c, first)
    e._on_new_building()
    _draw(c, second)
    return app, w, e, c, (0 if big_first else 1), (1 if big_first else 0)


def test_inner_node_is_grabbable_when_big_drawn_first(tmp_path):
    app, w, e, c, big_i, small_i = _nested_editor(tmp_path, big_first=True)
    e.active_tool = "editPoints"
    px, py = c._to_screen(4, 3)              # corner of the SMALL building
    c._handle_editpoints_press(px, py)
    assert c.active_index() == small_i
    assert c._drag_idx == c._buildings[small_i].vertices.index((4.0, 3.0))
    assert c._move_anchor is None


def test_inner_node_is_grabbable_when_big_drawn_last(tmp_path):
    # The reported case: the outer building is on top (drawn later), yet the
    # inner building's corner must still win the grab.
    app, w, e, c, big_i, small_i = _nested_editor(tmp_path, big_first=False)
    e.active_tool = "editPoints"
    px, py = c._to_screen(7, 5)
    c._handle_editpoints_press(px, py)
    assert c.active_index() == small_i
    assert c._drag_idx is not None
    assert c._buildings[small_i].vertices[c._drag_idx] == (7.0, 5.0)


def test_click_inside_inner_moves_inner_not_outer(tmp_path):
    app, w, e, c, big_i, small_i = _nested_editor(tmp_path, big_first=False)
    e.active_tool = "editPoints"
    px, py = c._to_screen(5.5, 4)            # centre of SMALL, far from nodes
    c._handle_editpoints_press(px, py)
    assert c.active_index() == small_i       # smallest containing building wins
    assert c._move_anchor is not None


def test_click_outside_inner_still_selects_outer(tmp_path):
    app, w, e, c, big_i, small_i = _nested_editor(tmp_path, big_first=True)
    e.active_tool = "editPoints"
    px, py = c._to_screen(10.5, 6.9)         # inside BIG only, off its nodes
    c._handle_editpoints_press(px, py)
    assert c.active_index() == big_i
    assert c._move_anchor is not None


def test_select_building_at_prefers_smallest(tmp_path):
    app, w, e, c, big_i, small_i = _nested_editor(tmp_path, big_first=False)
    px, py = c._to_screen(5.5, 4)
    assert c.select_building_at(px, py) == small_i
    px, py = c._to_screen(1, 1)
    assert c.select_building_at(px, py) == big_i


def test_outer_vertex_drag_still_works(tmp_path):
    app, w, e, c, big_i, small_i = _nested_editor(tmp_path, big_first=True)
    e.active_tool = "editPoints"
    px, py = c._to_screen(0, 0)              # corner of BIG
    c._handle_editpoints_press(px, py)
    assert c.active_index() == big_i
    assert c._drag_idx is not None
