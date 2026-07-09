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


def test_gable_on_long_edges_rotates_the_ridge(tmp_path):
    """On a rectangle a gable is allowed on ANY edge: gabling both LONG edges
    turns the roof into a sedlová with the ridge along the SHORT axis."""
    app, w, ed = _editor(tmp_path)
    c = ed.canvas
    _rect(c)                                # (2,2)–(12,8): 10 × 6
    ed._on_calculate()
    toasts = []
    ed._show_toast = lambda t: toasts.append(t)
    # Click near the long bottom edge → it becomes a gable (3-eave interim
    # state cannot compute yet, but the toggle itself must be allowed).
    px, py = c._to_screen(7, 2.5)
    c._handle_eavegable_click(px, py)
    assert c._edge_types[0] == "gable"
    # And the opposite long edge → a rotated sedlová that computes again.
    px, py = c._to_screen(7, 7.5)
    c._handle_eavegable_click(px, py)
    assert c._edge_types[2] == "gable"
    ed._on_calculate()
    result = c._skeleton_data
    assert result is not None
    assert abs(result.ridge_length_m - 6.0) < 1e-6      # short axis
    ridge = [s for s in result.skeleton_segments if s[0] == "ridge"][0]
    (_k, a, b) = ridge
    assert abs(a[0] - 7.0) < 1e-6 and abs(b[0] - 7.0) < 1e-6   # x = centre
    assert {round(a[1], 6), round(b[1], 6)} == {2.0, 8.0}


def test_uncomputable_gable_combo_toasts_and_keeps_running(tmp_path):
    """One gable on a LONG edge (3 eaves) has no roof type yet — the calc
    must fail with a toast, and the 3D view must not crash on it."""
    app, w, ed = _editor(tmp_path)
    c = ed.canvas
    _rect(c)
    ed._on_calculate()
    toasts = []
    ed._show_toast = lambda t: toasts.append(t)
    px, py = c._to_screen(7, 2.5)
    c._handle_eavegable_click(px, py)      # live recompute fails inside
    assert c._edge_types[0] == "gable"
    assert c._skeleton_data is None        # no roof for this combination
    assert toasts                          # unsupported-shape toast shown
    ed._update_3d_all()                    # walls-only scene, no exception


def test_click_outside_polygon_does_nothing(tmp_path):
    app, w, ed = _editor(tmp_path)
    c = ed.canvas
    _rect(c)
    ed._on_calculate()
    px, py = c._to_screen(50, 50)   # far outside
    c._handle_eavegable_click(px, py)
    assert c._edge_types == ["eave"] * 4
