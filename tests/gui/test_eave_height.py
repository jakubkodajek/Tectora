"""Single per-building height (Výška okapu): model, canvas, 3D, editor sync.

The vertical model is ground-aligned: the shared ground plane is z=0 and a
building's walls span [0, eave_height]. Two overlapping buildings with
different eave heights therefore stand on the same ground with one rising
above the other. Legacy files carried wall_height_m (+ base_height_m); loading
sums them into eave_height_m.
"""
import os
os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")


def _app():
    from PySide6.QtWidgets import QApplication
    return QApplication.instance() or QApplication([])


def _editor(tmp_path):
    from tectora.application.project import Project
    from tectora.gui.app_window import MainWindow
    from tectora.io.sqlite_library import SqliteProjectLibrary
    app = _app()
    lib = SqliteProjectLibrary(str(tmp_path / "lib.db"))
    w = MainWindow(library=lib)
    w.open_editor(Project(global_pitch_deg=35.0))
    return app, w, w._stack.widget(1)


RECT = [(0, 0), (10, 0), (10, 6), (0, 6)]


# ---------------------------------------------------------------------------
# Model serialization + legacy compatibility
# ---------------------------------------------------------------------------

def test_building_eave_height_roundtrip():
    from tectora.application.project import Building
    b = Building(eave_height_m=3.5)
    assert Building.from_dict(b.to_dict()).eave_height_m == 3.5


def test_building_to_dict_has_no_legacy_height_keys():
    from tectora.application.project import Building
    d = Building(eave_height_m=4.0).to_dict()
    assert "wall_height_m" not in d and "base_height_m" not in d
    assert d["eave_height_m"] == 4.0


def test_legacy_wall_plus_base_becomes_eave_height():
    from tectora.application.project import Building
    b = Building.from_dict({
        "footprint": None, "pitch_deg": 35.0,
        "wall_height_m": 2.5, "base_height_m": 3.0,
    })
    assert b.eave_height_m == 5.5


def test_missing_height_keys_default_to_2_5():
    from tectora.application.project import Building
    b = Building.from_dict({"footprint": None, "pitch_deg": 35.0})
    assert b.eave_height_m == 2.5


def test_project_wall_height_alias_still_works():
    from tectora.application.project import Project
    p = Project(project_name="X", wall_height_m=3.0)
    assert p.eave_height_m == 3.0
    assert Project.from_dict(p.to_dict()).wall_height_m == 3.0


# ---------------------------------------------------------------------------
# 3D: walls span ground → eave
# ---------------------------------------------------------------------------

def _footprint(pts):
    from tectora.core.footprint import Footprint
    return Footprint.from_lists(list(pts), ["eave"] * len(pts))


def test_walls_span_ground_to_eave():
    from tectora.gui.editor.view_3d import Roof3DView
    _app()
    v = Roof3DView()
    v.set_design([(_footprint(RECT), 35, 5.5, 0.0)])
    wall_zs = [vv[2] for (verts, is_wall) in v._geom_faces if is_wall for vv in verts]
    roof_zs = [vv[2] for (verts, is_wall) in v._geom_faces if not is_wall for vv in verts]
    assert abs(min(wall_zs) - 0.0) < 1e-9          # wall base on the ground
    assert abs(max(wall_zs) - 5.5) < 1e-9          # wall top = eave level
    assert abs(min(roof_zs) - 5.5) < 1e-9          # roof starts on the wall top


def test_two_buildings_with_different_eaves_share_one_scene():
    from tectora.gui.editor.view_3d import Roof3DView
    _app()
    low = _footprint(RECT)
    high = _footprint([(2, 2), (6, 2), (6, 5), (2, 5)])
    v = Roof3DView()
    v.set_design([
        (low, 35, 2.5, 0.0, None),
        (high, 35, 6.5, 0.0, None),
    ])
    zs = [vv[2] for (verts, _w) in v._geom_faces for vv in verts]
    assert abs(min(zs) - 0.0) < 1e-9   # both stand on the ground
    assert max(zs) > 6.5               # the tall one rises above its eave


# ---------------------------------------------------------------------------
# Canvas: accessors + undo snapshot
# ---------------------------------------------------------------------------

def test_canvas_eave_height_survives_undo(tmp_path):
    app, w, e = _editor(tmp_path)
    c = e.canvas
    c.set_active_eave_height(4.0)
    c.add_point(0, 0)                  # pushes an undo snapshot (eave = 4)
    c.set_active_eave_height(7.0)
    assert c.active_eave_height() == 7.0
    assert c.undo()
    assert c.active_eave_height() == 4.0
    assert len(c._vertices) == 0


# ---------------------------------------------------------------------------
# Editor: panel sync + save/restore
# ---------------------------------------------------------------------------

def test_eave_height_spin_follows_active_building(tmp_path):
    app, w, e = _editor(tmp_path)
    c = e.canvas
    for (x, y) in RECT:
        c.add_point(x, y)
    c.close_polygon()
    e._eave_height_spin.setValue(2.0)
    assert c.active_eave_height() == 2.0

    e._on_new_building()
    for (x, y) in [(20, 0), (26, 0), (26, 4), (20, 4)]:
        c.add_point(x, y)
    c.close_polygon()
    e._eave_height_spin.setValue(5.0)
    assert c.active_eave_height() == 5.0

    c.set_active(0)
    assert e._eave_height_spin.value() == 2.0
    c.set_active(1)
    assert e._eave_height_spin.value() == 5.0


def test_collect_buildings_carries_eave_height(tmp_path):
    app, w, e = _editor(tmp_path)
    c = e.canvas
    for (x, y) in RECT:
        c.add_point(x, y)
    c.close_polygon()
    e._eave_height_spin.setValue(3.0)
    buildings = e._collect_buildings()
    assert len(buildings) == 1
    assert buildings[0].eave_height_m == 3.0


def test_load_project_restores_eave_height(tmp_path):
    from tectora.application.project import Building, Project
    from tectora.core.footprint import Footprint
    app, w, e = _editor(tmp_path)
    fp = Footprint.from_lists(RECT, ["eave"] * 4)
    proj = Project(project_name="Věž", buildings=[
        Building(footprint=fp, pitch_deg=35.0, eave_height_m=6.0),
        Building(footprint=Footprint.from_lists(
            [(20, 0), (26, 0), (26, 4), (20, 4)], ["eave"] * 4), pitch_deg=30.0),
    ])
    e.load_project(proj)
    assert e.canvas._buildings[0].eave_height == 6.0
    assert e.canvas._buildings[1].eave_height == 2.5
    assert e._eave_height_spin.value() == 6.0   # panel synced to active (first)


def test_eave_hover_shows_the_single_height(tmp_path):
    from PySide6.QtGui import QPixmap
    app, w, e = _editor(tmp_path)
    c = e.canvas
    c.resize(700, 460)
    for (x, y) in RECT:
        c.add_point(x, y)
    c.close_polygon()
    e._eave_height_spin.setValue(5.5)
    e._on_calculate()
    pm = QPixmap(c.size())
    c.render(pm)
    ex, ey = c._to_screen(5, 0)        # midpoint of the bottom eave edge
    text = c._hover_text_at(ex, ey)
    assert text is not None and text.endswith("5.50 m")
