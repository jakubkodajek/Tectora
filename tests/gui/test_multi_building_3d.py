"""P4a: the 3D view renders all buildings of a design via set_design()."""
import os
os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")


def _app():
    from PySide6.QtWidgets import QApplication
    return QApplication.instance() or QApplication([])


def test_set_design_renders_all_buildings():
    from PySide6.QtGui import QPixmap
    from tectora.core.footprint import Footprint
    from tectora.gui.editor.view_3d import Roof3DView
    _app()
    fp1 = Footprint.from_lists([(0, 0), (8, 0), (8, 5), (0, 5)], ["eave"] * 4)
    fp2 = Footprint.from_lists([(20, 0), (26, 0), (26, 4), (20, 4)], ["eave"] * 4)

    v = Roof3DView()
    v.set_geometry(fp1, 35, 2.5, 0.0)
    one = len(v._geom_faces)
    v.set_design([(fp1, 35, 2.5, 0.0), (fp2, 22, 2.5, 0.0)])
    two = len(v._geom_faces)
    # Two buildings produce strictly more faces than one (both are drawn).
    assert two > one
    # A None footprint in the list is skipped, not crashed on.
    v.set_design([(fp1, 35, 2.5, 0.0), (None, 0, 0, 0)])
    assert len(v._geom_faces) == one

    v.resize(400, 300)
    pm = QPixmap(v.size())
    v.render(pm)
    assert not pm.isNull()


def test_set_geometry_single_building_unchanged():
    from tectora.core.footprint import Footprint
    from tectora.gui.editor.view_3d import Roof3DView
    _app()
    fp = Footprint.from_lists([(0, 0), (8, 0), (8, 5), (0, 5)], ["eave"] * 4)
    v = Roof3DView()
    v.set_geometry(fp, 35, 2.5, 0.0)
    # 4 wall quads + roof faces (the rectangle hip has 4 roof planes).
    walls = sum(1 for _verts, is_wall in v._geom_faces if is_wall)
    assert walls == 4
    v.set_geometry(None, 35, 2.5, 0.0)
    assert v._geom_faces == []
