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
    return app, w._stack.widget(1)


def test_project_has_wall_height_roundtrip():
    from tectora.application.project import Project
    p = Project(project_name="X", wall_height_m=3.0)
    assert Project.from_dict(p.to_dict()).wall_height_m == 3.0


def test_set_geometry_renders_without_crash(tmp_path):
    from tectora.core.footprint import Footprint
    app, e = _editor(tmp_path)
    fp = Footprint.from_lists([(0, 0), (12, 0), (12, 8), (0, 8)], ["eave"] * 4)
    e.view3d.set_geometry(fp, 35, 2.5)
    e.view3d.resize(400, 300)
    e.view3d.repaint()
    app.processEvents()


def test_calculate_populates_3d_with_walls(tmp_path):
    app, e = _editor(tmp_path)
    for (x, y) in [(0, 0), (12, 0), (12, 8), (0, 8)]:
        e.canvas.add_point(x, y)
    e.canvas.close_polygon()
    e._on_calculate()
    app.processEvents()
    assert e.last_result is not None
    # 3D geometry available (walls + roof); no assertion on pixels, just no crash
    e.view3d.resize(400, 300)
    e.view3d.repaint()
    app.processEvents()


def test_set_geometry_overhang_insets_walls(tmp_path):
    from tectora.core.footprint import Footprint
    from tectora.gui.editor.view_3d import Roof3DView
    from PySide6.QtWidgets import QApplication
    QApplication.instance() or QApplication([])
    fp = Footprint.from_lists([(0, 0), (10, 0), (10, 10), (0, 10)], ["eave"] * 4)
    v = Roof3DView()
    v.set_geometry(fp, 35, 2.5, overhang=0.0)
    roof0 = [vv for (verts, is_wall) in v._geom_faces if not is_wall for vv in verts]
    walls0 = [vv for (verts, is_wall) in v._geom_faces if is_wall for vv in verts]
    v.set_geometry(fp, 35, 2.5, overhang=1.0)
    roof1 = [vv for (verts, is_wall) in v._geom_faces if not is_wall for vv in verts]
    walls1 = [vv for (verts, is_wall) in v._geom_faces if is_wall for vv in verts]
    # We model the ROOF: the drawn eave is unchanged by overhang; the WALLS move
    # inward (the building sits under the roof edge).
    assert max(x for (x, _y, _z) in roof1) == max(x for (x, _y, _z) in roof0)
    assert max(x for (x, _y, _z) in walls1) < max(x for (x, _y, _z) in walls0)
