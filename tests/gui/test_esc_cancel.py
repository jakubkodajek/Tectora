"""Esc cancels the current action — like in other drawing apps.

Covers: discarding the in-progress polyline, closing the inline length /
pitch editors WITHOUT applying the typed value, and reverting a node drag.
"""
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


def _press_escape(widget):
    from PySide6.QtCore import Qt, QEvent
    from PySide6.QtGui import QKeyEvent
    from PySide6.QtWidgets import QApplication
    ev = QKeyEvent(QEvent.Type.KeyPress, Qt.Key.Key_Escape,
                   Qt.KeyboardModifier.NoModifier)
    QApplication.sendEvent(widget, ev)


def test_esc_parks_in_progress_polyline(tmp_path):
    """Esc finishes the polyline like in CAD: it stays as a loose chain and
    the next click starts a new one."""
    app, w, ed = _editor(tmp_path)
    c = ed.canvas
    c.add_point(0, 0)
    c.add_point(5, 0)
    _press_escape(c)
    assert c._vertices == []
    assert not c._closed
    assert c._buildings[0].extra_chains == [[(0.0, 0.0), (5.0, 0.0)]]
    # Undoable: the chain returns to being the active polyline.
    assert c.undo()
    assert c._vertices == [(0.0, 0.0), (5.0, 0.0)]
    assert c._buildings[0].extra_chains == []


def test_esc_discards_a_single_stray_point(tmp_path):
    app, w, ed = _editor(tmp_path)
    c = ed.canvas
    c.add_point(0, 0)
    _press_escape(c)
    assert c._vertices == []
    assert c._buildings[0].extra_chains == []


def test_esc_on_closed_polygon_does_nothing(tmp_path):
    app, w, ed = _editor(tmp_path)
    c = ed.canvas
    for x, y in [(0, 0), (5, 0), (5, 3), (0, 3)]:
        c.add_point(x, y)
    c.close_polygon()
    before = list(c._vertices)
    _press_escape(c)
    assert c._vertices == before
    assert c._closed


def test_esc_closes_length_editor_without_applying(tmp_path):
    app, w, ed = _editor(tmp_path)
    c = ed.canvas
    for x, y in [(0, 0), (5, 0), (5, 3), (0, 3)]:
        c.add_point(x, y)
    c.close_polygon()
    before = list(c._vertices)
    c._open_len_editor(0, 0, 5.0, 100.0, 100.0)
    c._len_editor.setText("9")
    _press_escape(c._len_editor)   # propagates to the canvas handler
    app.processEvents()
    assert c._len_editor is None
    assert c._vertices == before   # the typed 9 m was NOT applied


def test_esc_closes_pitch_editor_without_applying(tmp_path):
    app, w, ed = _editor(tmp_path)
    c = ed.canvas
    for x, y in [(0, 0), (8, 0), (8, 5), (0, 5)]:
        c.add_point(x, y)
    c.close_polygon()
    ed._on_calculate()
    pitch_before = c.active_pitch()
    c._open_pitch_editor("Jižní", 35.0, 100.0, 100.0)
    c._pitch_editor.setText("50")
    _press_escape(c._pitch_editor)
    app.processEvents()
    assert c._pitch_editor is None
    assert c.active_pitch() == pitch_before


def test_esc_cancels_node_drag(tmp_path):
    app, w, ed = _editor(tmp_path)
    c = ed.canvas
    for x, y in [(0, 0), (5, 0), (5, 3), (0, 3)]:
        c.add_point(x, y)
    c.close_polygon()
    ed.active_tool = "editPoints"
    before = list(c._vertices)
    # Grab the node at (0, 0) and drag it.
    sx, sy = c._to_screen(0.0, 0.0)
    c._handle_editpoints_press(sx, sy)
    assert c._drag_idx is not None
    c.move_vertex(c._drag_idx, 1.0, 1.0)
    assert c._vertices != before
    _press_escape(c)
    assert c._drag_idx is None
    assert c._vertices == before   # drag reverted to its start
