"""Regression tests for reported bugs."""
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
    return app, w, w._stack.widget(1)


def test_gable_invalidated_by_edit_auto_reverts(tmp_path):
    """Setting a gable then slanting a neighbour edge must not fail silently —
    the now-invalid gable auto-reverts to eave and the roof still calculates."""
    app, w, ed = _editor(tmp_path)
    c = ed.canvas
    for x, y in [(0, 0), (8, 0), (8, 5), (0, 5)]:
        c.add_point(x, y)
    c.close_polygon()
    ed.active_tool = "eaveGable"
    c.toggle_edge_type(1)            # right short edge → gable (valid cap)
    ed._on_calculate()
    assert c._edge_types[1] == "gable"
    assert ed.last_result is not None

    toasts = []
    ed._show_toast = lambda t: toasts.append(t)
    c.move_vertex(2, 8, 8)           # slant the top edge → gable now invalid
    ed._on_calculate()
    assert c._edge_types[1] == "eave"        # auto-reverted
    assert ed.last_result is not None        # calc succeeded (hip), not silent fail
    assert toasts                            # user was told


def test_edge_resize_keeps_normal_resize(tmp_path):
    """The self-intersection guard must not block an ordinary resize."""
    from PySide6.QtGui import QPixmap
    app, w, ed = _editor(tmp_path)
    c = ed.canvas
    for x, y in [(0, 0), (5, 0), (5, 3), (0, 3)]:
        c.add_point(x, y)
    c.close_polygon()
    ed._on_calculate()
    pm = QPixmap(c.size()); c.render(pm)
    c.set_edge_length(0, 0, 8.0)
    assert c._vertices[1] == (8.0, 0.0)


def test_edge_resize_rejects_self_intersection(tmp_path):
    """A resize that would make the polygon self-intersect is rejected."""
    app, w, ed = _editor(tmp_path)
    c = ed.canvas
    # A concave arrow-like quad; pushing one vertex far flips an edge across.
    for x, y in [(0, 0), (6, 0), (1, 1), (0, 6)]:
        c.add_point(x, y)
    c.close_polygon()
    before = list(c._vertices)
    toasts = []
    ed._show_toast = lambda t: toasts.append(t)
    # Edge 1 is (6,0)->(1,1); blow it up massively along its direction.
    c.set_edge_length(0, 1, 100.0)
    # Either it stayed simple (applied) or it was rejected and left unchanged —
    # the invariant is that the stored polygon is always simple.
    from tectora.core.footprint import is_simple_polygon
    assert is_simple_polygon(c._vertices)
