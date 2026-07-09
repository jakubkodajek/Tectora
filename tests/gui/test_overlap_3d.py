"""Overlapping buildings merge in 3D and deduct the covered roof area.

A tower (higher eave) standing over a lower building has its walls cut at
the lower roof surface — no walls piercing down through the building below.
The editor's calculation deducts the covered part of the lower roof from the
design total and says so in a toast.
"""
import math
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
    w.open_editor(Project(project_name="Test", global_pitch_deg=35.0))
    e = w._stack.widget(1)
    e.canvas.resize(700, 460)
    return app, w, e


def _footprint(pts):
    from tectora.core.footprint import Footprint
    return Footprint.from_lists(list(pts), ["eave"] * len(pts))


BIG = [(0, 0), (20, 0), (20, 10), (0, 10)]
TOWER = [(4, 1), (8, 1), (8, 3), (4, 3)]


def test_roof_z_at_reads_the_surface():
    from tectora.core.roof_generator import generate_roof
    from tectora.gui.editor.view_3d import roof_z_at
    r = generate_roof(_footprint(BIG), 35)
    # On the front slope at (6, 2): z = 2·tan(35°) above the eave plane.
    z = roof_z_at(r.faces, 6.0, 2.0)
    assert z is not None
    assert abs(z - 2.0 * math.tan(math.radians(35))) < 1e-6
    assert roof_z_at(r.faces, 50.0, 50.0) is None      # outside the roof


def test_tower_walls_start_on_the_lower_roof():
    from tectora.core.roof_generator import generate_roof
    from tectora.gui.editor.view_3d import Roof3DView
    _app()
    big_fp, tower_fp = _footprint(BIG), _footprint(TOWER)
    big_faces = generate_roof(big_fp, 35).faces
    tower_faces = generate_roof(tower_fp, 35).faces
    v = Roof3DView()
    v.set_design([
        (big_fp, 35, 2.5, 0.0, big_faces),
        (tower_fp, 35, 6.0, 0.0, tower_faces),
    ])
    # Split the scene: the tower's wall quads are those with top z = 6.
    tower_wall_zs = [
        vv[2] for (verts, is_wall) in v._geom_faces if is_wall
        and any(abs(p[2] - 6.0) < 1e-9 for p in verts)
        for vv in verts
    ]
    big_wall_zs = [
        vv[2] for (verts, is_wall) in v._geom_faces if is_wall
        and any(abs(p[2] - 2.5) < 1e-9 for p in verts)
        for vv in verts
    ]
    assert big_wall_zs and abs(min(big_wall_zs) - 0.0) < 1e-9
    # The tower stands INSIDE the big footprint → its wall bottoms sit on the
    # big roof (≥ eave 2.5 + min slope rise at y ∈ [1, 3]), never on the ground.
    assert tower_wall_zs
    min_bottom = min(tower_wall_zs)
    assert min_bottom > 2.5 + 1.0 * math.tan(math.radians(35)) - 1e-6


def test_disjoint_buildings_keep_ground_walls():
    from tectora.gui.editor.view_3d import Roof3DView
    _app()
    a = _footprint([(0, 0), (10, 0), (10, 6), (0, 6)])
    b = _footprint([(30, 0), (36, 0), (36, 4), (30, 4)])
    v = Roof3DView()
    v.set_design([(a, 35, 2.5, 0.0, None), (b, 35, 6.0, 0.0, None)])
    wall_zs = [vv[2] for (verts, is_wall) in v._geom_faces if is_wall for vv in verts]
    assert abs(min(wall_zs) - 0.0) < 1e-9


def test_editor_deducts_overlap_and_toasts(tmp_path):
    app, w, e = _editor(tmp_path)
    c = e.canvas
    for (x, y) in BIG:
        c.add_point(x, y)
    c.close_polygon()
    e._on_new_building()
    for (x, y) in TOWER:
        c.add_point(x, y)
    c.close_polygon()
    e._eave_height_spin.setValue(6.0)

    toasts = []
    e._show_toast = lambda t: toasts.append(t)
    e._on_calculate()

    assert any("odečteno" in t for t in toasts)
    # The displayed design total subtracts the covered roof: tower plan 4×2
    # over one face of the 35° hip → 8/cos(35°).
    expected_deduction = 8.0 / math.cos(math.radians(35))
    per_building = sum(
        b.skeleton_data.total_area_m2 for b in c._buildings
        if b.skeleton_data is not None
    )
    shown = float(
        e._result_value_labels["area"].text().replace(",", ".").split()[0])
    assert abs(shown - (per_building - expected_deduction)) < 0.05
