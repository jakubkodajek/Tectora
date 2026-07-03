"""Exact-calculator roof faces must land ON the drawn building in 3D.

Regression: with polovalba or zadní sklon the 3D view received the exact
calculator's faces verbatim (calc frame: origin corner, length along x), so a
building drawn away from the origin — or taller than wide — rendered its roof
detached from the walls ("střecha se utrhla").
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
    return app, w, w._stack.widget(1)


def _draw(c, pts):
    for (x, y) in pts:
        c.add_point(x, y)
    c.close_polygon()


def _gable_caps(c):
    """Mark the two SHORT edges of the drawn rectangle as gables."""
    n = len(c._vertices)
    lens = sorted(
        (((c._vertices[(i + 1) % n][0] - c._vertices[i][0]) ** 2
          + (c._vertices[(i + 1) % n][1] - c._vertices[i][1]) ** 2), i)
        for i in range(n)
    )
    for (_l, i) in lens[:2]:
        c.toggle_edge_type(i)


def _roof_xy(e):
    return [(v[0], v[1])
            for (verts, is_wall) in e.view3d._geom_faces if not is_wall
            for v in verts]


def _assert_inside(pts, x0, y0, x1, y1, eps=1e-6):
    assert pts
    for (x, y) in pts:
        assert x0 - eps <= x <= x1 + eps and y0 - eps <= y <= y1 + eps, (
            f"roof vertex ({x}, {y}) is outside the drawn footprint "
            f"({x0}, {y0})–({x1}, {y1})")


def test_back_pitch_roof_sits_on_offset_building(tmp_path):
    app, w, e = _editor(tmp_path)
    c = e.canvas
    _draw(c, [(20, 10), (30, 10), (30, 16), (20, 16)])   # far from the origin
    _gable_caps(c)
    e._pitch_back_spin.setValue(20)
    e._on_calculate()
    _assert_inside(_roof_xy(e), 20, 10, 30, 16)


def test_half_hip_roof_sits_on_offset_building(tmp_path):
    app, w, e = _editor(tmp_path)
    c = e.canvas
    _draw(c, [(-14, 6), (-4, 6), (-4, 12), (-14, 12)])   # negative quadrant
    _gable_caps(c)
    e._half_hip_spin.setValue(1.0)
    e._on_calculate()
    _assert_inside(_roof_xy(e), -14, 6, -4, 12)


def test_portrait_building_roof_follows_swapped_axes(tmp_path):
    app, w, e = _editor(tmp_path)
    c = e.canvas
    # Taller than wide: the calculator puts the LONG side on its x-axis, so
    # the mapping must swap axes back onto the drawn footprint.
    _draw(c, [(0, 0), (6, 0), (6, 16), (0, 16)])
    _gable_caps(c)
    e._pitch_back_spin.setValue(22)
    e._on_calculate()
    _assert_inside(_roof_xy(e), 0, 0, 6, 16)


def test_roof_still_matches_walls_after_pitch_edit(tmp_path):
    """The reported flow: set a back pitch, then change the pitch again."""
    app, w, e = _editor(tmp_path)
    c = e.canvas
    _draw(c, [(20, 10), (30, 10), (30, 16), (20, 16)])
    _gable_caps(c)
    e._pitch_back_spin.setValue(20)
    e._on_calculate()
    e._pitch_spin.setValue(45)          # triggers recompute + 3D rebuild
    _assert_inside(_roof_xy(e), 20, 10, 30, 16)
