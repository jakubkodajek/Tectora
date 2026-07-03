"""Whole-building drag: grab inside the polygon (Upravit body) and move it."""
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


RECT = [(0, 0), (10, 0), (10, 6), (0, 6)]


def _draw(c, pts=RECT):
    for (x, y) in pts:
        c.add_point(x, y)
    c.close_polygon()


def test_translate_building_moves_all_vertices(tmp_path):
    app, w, e = _editor(tmp_path)
    c = e.canvas
    _draw(c)
    c.translate_building(0, 2.0, -1.5)
    assert c._vertices == [(2.0, -1.5), (12.0, -1.5), (12.0, 4.5), (2.0, 4.5)]
    assert c._skeleton_data is None      # roof must recompute after a move


def test_grab_inside_polygon_starts_building_drag(tmp_path):
    app, w, e = _editor(tmp_path)
    c = e.canvas
    _draw(c)
    e.active_tool = "editPoints"
    px, py = c._to_screen(5, 3)          # centre of the rectangle, not a node
    c._handle_editpoints_press(px, py)
    assert c._drag_idx is None           # no vertex grabbed
    assert c._move_anchor is not None    # whole-building drag armed


def test_grab_on_node_still_drags_the_vertex(tmp_path):
    app, w, e = _editor(tmp_path)
    c = e.canvas
    _draw(c)
    e.active_tool = "editPoints"
    px, py = c._to_screen(0, 0)          # exactly on a corner node
    c._handle_editpoints_press(px, py)
    assert c._drag_idx == 0
    assert c._move_anchor is None


def test_building_drag_is_one_undo_step(tmp_path):
    app, w, e = _editor(tmp_path)
    c = e.canvas
    _draw(c)
    original = list(c._vertices)
    e.active_tool = "editPoints"
    px, py = c._to_screen(5, 3)
    c._handle_editpoints_press(px, py)   # pushes ONE undo snapshot
    c.translate_building(0, 1.0, 0.0)    # simulated drag steps
    c.translate_building(0, 1.0, 0.5)
    assert c._vertices != original
    assert c.undo()
    assert c._vertices == original       # whole drag reverted in one step
