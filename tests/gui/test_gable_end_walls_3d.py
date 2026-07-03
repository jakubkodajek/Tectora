"""Provided roof faces (asymmetric gable, half-hip) get vertical end walls.

Before, feeding precomputed faces into the 3D view skipped gable walls
entirely — the roof ends rendered hollow. skirt_walls closes every non-eave
boundary edge down to the eave plane.
"""
import os
os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

RECT = [(0, 0), (10, 0), (10, 6), (0, 6)]


def _app():
    from PySide6.QtWidgets import QApplication
    return QApplication.instance() or QApplication([])


def _gabled_footprint(pts=RECT):
    from tectora.core.footprint import Footprint
    n = len(pts)
    lens = []
    for i in range(n):
        ax, ay = pts[i]
        bx, by = pts[(i + 1) % n]
        lens.append(((bx - ax) ** 2 + (by - ay) ** 2, i))
    lens.sort()
    types = ["eave"] * n
    for (_l, i) in lens[:2]:
        types[i] = "gable"                    # both short caps → GABLE spec
    return Footprint.from_lists(pts, types)


def test_asymmetric_gable_has_end_walls():
    from tectora.core.roof_generator import generate_roof
    from tectora.gui.editor.view_3d import Roof3DView

    _app()
    fp = _gabled_footprint()
    result = generate_roof(fp, 35.0, pitch_back=20.0)
    assert result.faces                       # exact calculator provides faces

    v = Roof3DView()
    v.set_design([(fp, 35, 2.5, 0.0, result.faces)])
    walls = [verts for (verts, is_wall) in v._geom_faces if is_wall]
    # 4 house walls + skirt walls closing both gable ends.
    assert len(walls) > 4
    # A skirt reaches from the eave plane (z=2.5) up to the ridge (above it).
    skirts = [w for w in walls if any(vv[2] > 2.5 + 1e-6 for vv in w)]
    assert skirts
    assert all(min(vv[2] for vv in w) >= 2.5 - 1e-6 for w in skirts)


def test_hip_faces_get_no_skirts():
    """A hip roof's faces close themselves — every boundary edge is an eave."""
    from tectora.core.roof_generator import generate_roof
    from tectora.core.footprint import Footprint
    from tectora.gui.editor.view_3d import Roof3DView

    _app()
    fp = Footprint.from_lists(RECT, ["eave"] * 4)
    result = generate_roof(fp, 35.0)
    v = Roof3DView()
    v.set_design([(fp, 35, 2.5, 0.0, result.faces)])
    walls = [verts for (verts, is_wall) in v._geom_faces if is_wall]
    assert len(walls) == 4                    # just the house walls


def test_skirt_walls_are_vertical():
    from tectora.core.roof_generator import generate_roof
    from tectora.gui.editor.view_3d import skirt_walls

    fp = _gabled_footprint()
    result = generate_roof(fp, 35.0, pitch_back=20.0)
    for quad in skirt_walls(result.faces):
        # Vertical: the top edge projects exactly onto the bottom edge (z=0).
        (a, b, b0, a0) = quad
        assert (a[0], a[1]) == (a0[0], a0[1])
        assert (b[0], b[1]) == (b0[0], b0[1])
        assert a0[2] == 0.0 and b0[2] == 0.0
