"""Per-building base height (Výška budovy): model, canvas, 3D and editor sync.

The vertical model is ground-aligned: the shared ground plane is z=0, a
building's wall band spans [base, base + wall] and its roof sits on the wall
tops. Raising ``base_height_m`` lifts the whole building, so two overlapping
buildings can sit at different levels (e.g. a tower over a lower wing).
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
# Model serialization
# ---------------------------------------------------------------------------

def test_building_base_height_roundtrip():
    from tectora.application.project import Building
    b = Building(base_height_m=3.5)
    assert Building.from_dict(b.to_dict()).base_height_m == 3.5


def test_building_base_height_defaults_to_zero_for_old_files():
    from tectora.application.project import Building
    # Schema written before the field existed has no key at all.
    b = Building.from_dict({"footprint": None, "pitch_deg": 35.0})
    assert b.base_height_m == 0.0


# ---------------------------------------------------------------------------
# 3D: ground-aligned scene + per-building lift
# ---------------------------------------------------------------------------

def _footprint(pts):
    from tectora.core.footprint import Footprint
    return Footprint.from_lists(list(pts), ["eave"] * len(pts))


def test_walls_stand_on_the_ground_plane():
    from tectora.gui.editor.view_3d import Roof3DView
    _app()
    v = Roof3DView()
    v.set_design([(_footprint(RECT), 35, 2.5, 0.0)])
    wall_zs = [vv[2] for (verts, is_wall) in v._geom_faces if is_wall for vv in verts]
    roof_zs = [vv[2] for (verts, is_wall) in v._geom_faces if not is_wall for vv in verts]
    assert abs(min(wall_zs) - 0.0) < 1e-9          # wall base on the ground
    assert abs(max(wall_zs) - 2.5) < 1e-9          # wall top = eave level
    assert abs(min(roof_zs) - 2.5) < 1e-9          # roof starts on the wall top


def test_base_height_lifts_the_whole_building():
    from tectora.gui.editor.view_3d import Roof3DView
    _app()
    fp = _footprint(RECT)
    v = Roof3DView()
    v.set_design([(fp, 35, 2.5, 0.0)])
    z0 = sorted(vv[2] for (verts, _w) in v._geom_faces for vv in verts)
    v.set_design([(fp, 35, 2.5, 0.0, None, 3.0)])
    z3 = sorted(vv[2] for (verts, _w) in v._geom_faces for vv in verts)
    assert len(z0) == len(z3)
    assert all(abs(b - a - 3.0) < 1e-9 for a, b in zip(z0, z3))


def test_two_buildings_at_different_levels_share_one_scene():
    from tectora.gui.editor.view_3d import Roof3DView
    _app()
    low = _footprint(RECT)
    high = _footprint([(2, 2), (6, 2), (6, 5), (2, 5)])
    v = Roof3DView()
    v.set_design([
        (low, 35, 2.5, 0.0, None, 0.0),
        (high, 35, 2.5, 0.0, None, 4.0),
    ])
    zs = [vv[2] for (verts, _w) in v._geom_faces for vv in verts]
    assert abs(min(zs) - 0.0) < 1e-9   # the low building stands on the ground
    assert max(zs) > 6.5               # the high one reaches above 4 + 2.5


# ---------------------------------------------------------------------------
# Canvas: accessors + undo snapshot
# ---------------------------------------------------------------------------

def test_canvas_base_height_survives_undo(tmp_path):
    app, w, e = _editor(tmp_path)
    c = e.canvas
    c.set_active_base_height(4.0)
    c.add_point(0, 0)                  # pushes an undo snapshot (base = 4)
    c.set_active_base_height(7.0)
    assert c.active_base_height() == 7.0
    assert c.undo()
    assert c.active_base_height() == 4.0
    assert len(c._vertices) == 0


def test_canvas_base_height_never_negative(tmp_path):
    app, w, e = _editor(tmp_path)
    e.canvas.set_active_base_height(-2.0)
    assert e.canvas.active_base_height() == 0.0


# ---------------------------------------------------------------------------
# Editor: panel sync + save/restore
# ---------------------------------------------------------------------------

def test_base_height_spin_follows_active_building(tmp_path):
    app, w, e = _editor(tmp_path)
    c = e.canvas
    for (x, y) in RECT:
        c.add_point(x, y)
    c.close_polygon()
    e._base_height_spin.setValue(2.0)
    assert c.active_base_height() == 2.0

    e._on_new_building()
    for (x, y) in [(20, 0), (26, 0), (26, 4), (20, 4)]:
        c.add_point(x, y)
    c.close_polygon()
    e._base_height_spin.setValue(5.0)
    assert c.active_base_height() == 5.0

    c.set_active(0)
    assert e._base_height_spin.value() == 2.0
    c.set_active(1)
    assert e._base_height_spin.value() == 5.0


def test_collect_buildings_carries_base_height(tmp_path):
    app, w, e = _editor(tmp_path)
    c = e.canvas
    for (x, y) in RECT:
        c.add_point(x, y)
    c.close_polygon()
    e._base_height_spin.setValue(3.0)
    buildings = e._collect_buildings()
    assert len(buildings) == 1
    assert buildings[0].base_height_m == 3.0


def test_load_project_restores_base_height(tmp_path):
    from tectora.application.project import Building, Project
    from tectora.core.footprint import Footprint
    app, w, e = _editor(tmp_path)
    fp = Footprint.from_lists(RECT, ["eave"] * 4)
    proj = Project(project_name="Věž", buildings=[
        Building(footprint=fp, pitch_deg=35.0, base_height_m=6.0),
        Building(footprint=Footprint.from_lists(
            [(20, 0), (26, 0), (26, 4), (20, 4)], ["eave"] * 4), pitch_deg=30.0),
    ])
    e.load_project(proj)
    assert e.canvas._buildings[0].base_height == 6.0
    assert e.canvas._buildings[1].base_height == 0.0
    assert e._base_height_spin.value() == 6.0   # panel synced to active (first)


def test_eave_hover_reflects_base_height(tmp_path):
    from PySide6.QtGui import QPixmap
    app, w, e = _editor(tmp_path)
    c = e.canvas
    c.resize(700, 460)
    for (x, y) in RECT:
        c.add_point(x, y)
    c.close_polygon()
    e._base_height_spin.setValue(3.0)
    e._on_calculate()
    pm = QPixmap(c.size())
    c.render(pm)
    ex, ey = c._to_screen(5, 0)        # midpoint of the bottom eave edge
    text = c._hover_text_at(ex, ey)
    # base 3.0 + wall 2.5 → the eave line is 5.50 m above ground
    assert text is not None and text.endswith("5.50 m")
