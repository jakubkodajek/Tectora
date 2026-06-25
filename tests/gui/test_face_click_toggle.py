"""Clicking inside a roof face (not just on the eave line) toggles that edge
between eave and gable."""
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
    ed.active_tool = "eaveGable"
    return app, w, ed


def _rect(c):
    for x, y in [(2, 2), (12, 2), (12, 8), (2, 8)]:
        c.add_point(x, y)
    c.close_polygon()


def test_click_in_face_toggles_nearest_edge(tmp_path):
    app, w, ed = _editor(tmp_path)
    c = ed.canvas
    _rect(c)
    ed._on_calculate()
    assert c._edge_types == ["eave"] * 4
    # Click INSIDE the right cap face (model ~ (11, 5)), not on the line.
    px, py = c._to_screen(11, 5)
    c._handle_eavegable_click(px, py)
    assert c._edge_types[1] == "gable"   # the right short edge became a gable
    # Clicking the same face again toggles back.
    px, py = c._to_screen(11, 5)
    c._handle_eavegable_click(px, py)
    assert c._edge_types[1] == "eave"


def test_click_in_face_near_invalid_edge_is_rejected(tmp_path):
    app, w, ed = _editor(tmp_path)
    c = ed.canvas
    _rect(c)
    ed._on_calculate()
    toasts = []
    ed._show_toast = lambda t: toasts.append(t)
    # Near the long bottom edge — not a valid Stage-1 gable cap.
    px, py = c._to_screen(7, 2.5)
    c._handle_eavegable_click(px, py)
    assert c._edge_types == ["eave"] * 4   # unchanged
    assert toasts   # a "not here" toast was shown


def test_click_outside_polygon_does_nothing(tmp_path):
    app, w, ed = _editor(tmp_path)
    c = ed.canvas
    _rect(c)
    ed._on_calculate()
    px, py = c._to_screen(50, 50)   # far outside
    c._handle_eavegable_click(px, py)
    assert c._edge_types == ["eave"] * 4
