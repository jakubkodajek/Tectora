# -*- coding: utf-8 -*-
"""T3: 3D vertical gable wall (štít) at a valid wing-end of a general shape.

Offscreen GUI test. ``Roof3DView.set_geometry`` should add one vertical
triangular wall face per valid ``gable`` edge when there is no overhang, and
suppress them entirely under an overhang (the roof is rebuilt all-eave there).
"""
import os
os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

# An L-shaped footprint. Two of its edges are valid Stage-1 wing-end caps.
L_PTS = [(0, 0), (6, 0), (6, 3), (3, 3), (3, 6), (0, 6)]


def _app():
    from PySide6.QtWidgets import QApplication
    return QApplication.instance() or QApplication([])


def _wall_count(view) -> int:
    return sum(1 for (_verts, is_wall) in view._geom_faces if is_wall)


def _pick_valid_gable_index() -> int:
    """Index of one valid wing-end gable edge on the (normalised) L footprint.

    We let the domain predicate identify it rather than hard-coding an index,
    so the test stays correct regardless of how ``Footprint`` winds/rotates the
    edge list. The L has two valid caps; we take the first.
    """
    from tectora.core.footprint import Footprint
    from tectora.core.gable import is_valid_gable_edge
    fp = Footprint.from_lists(L_PTS, ["eave"] * len(L_PTS))
    for i in range(fp.n):
        if is_valid_gable_edge(fp, i):
            return i
    raise AssertionError("L footprint has no valid wing-end gable edge")


def test_gable_adds_one_vertical_wall_triangle():
    from tectora.core.footprint import Footprint
    from tectora.gui.editor.view_3d import Roof3DView

    _app()
    n = len(L_PTS)
    gi = _pick_valid_gable_index()

    fp_plain = Footprint.from_lists(L_PTS, ["eave"] * n)
    types = ["eave"] * n
    types[gi] = "gable"
    fp_gable = Footprint.from_lists(L_PTS, types)

    view = Roof3DView()

    view.set_geometry(fp_plain, 35, 2.5, 0.0)
    plain_walls = _wall_count(view)
    plain_faces = list(view._geom_faces)

    view.set_geometry(fp_gable, 35, 2.5, 0.0)
    gable_walls = _wall_count(view)
    gable_faces = list(view._geom_faces)

    # Exactly one extra wall face (the vertical štít).
    assert gable_walls == plain_walls + 1

    # The plain footprint's walls are all 4-vertex quads; the gabled one adds a
    # single 3-vertex wall (the štít). Identify it as the only triangular wall.
    # (Roof faces also change shape under the fold, so a plain set-difference
    # would over-count — we key on wall + triangle instead.)
    assert all(len(v) == 4 for (v, is_wall) in plain_faces if is_wall)
    tri_walls = [(v, w) for (v, w) in gable_faces if w and len(v) == 3]
    assert len(tri_walls) == 1
    tri_verts, _is_wall = tri_walls[0]
    zs = sorted(v[2] for v in tri_verts)
    assert abs(zs[0]) < 1e-9            # base corner A on the eave line
    assert abs(zs[1]) < 1e-9            # base corner B on the eave line
    assert zs[2] > 0.0                  # ridge apex M lifted above the edge


def test_overhang_keeps_gable_wall():
    from tectora.core.footprint import Footprint
    from tectora.gui.editor.view_3d import Roof3DView

    _app()
    n = len(L_PTS)
    gi = _pick_valid_gable_index()
    types = ["eave"] * n
    types[gi] = "gable"
    fp_gable = Footprint.from_lists(L_PTS, types)

    view = Roof3DView()
    view.set_geometry(fp_gable, 35, 2.5, overhang=1.0)

    # New semantics: the roof is the drawn polygon at any overhang, so the gable
    # is honoured — the vertical štít triangle is still drawn even with overhang.
    tri_walls = [(v, w) for (v, w) in view._geom_faces if w and len(v) == 3]
    assert len(tri_walls) == 1


def test_gabled_view_renders_without_exception():
    from PySide6.QtGui import QPixmap
    from tectora.core.footprint import Footprint
    from tectora.gui.editor.view_3d import Roof3DView

    app = _app()
    n = len(L_PTS)
    gi = _pick_valid_gable_index()
    types = ["eave"] * n
    types[gi] = "gable"
    fp_gable = Footprint.from_lists(L_PTS, types)

    view = Roof3DView()
    view.set_geometry(fp_gable, 35, 2.5, 0.0)
    view.resize(400, 300)
    pm = QPixmap(view.size())
    view.render(pm)              # exercises _draw_geom_scene incl. the štít
    app.processEvents()
    assert not pm.isNull()
