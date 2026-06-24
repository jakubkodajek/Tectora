import os

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")


def _canvas():
    from PySide6.QtWidgets import QApplication
    from tectora.gui.app_window import MainWindow
    from tectora.io.sqlite_library import SqliteProjectLibrary
    import tempfile, os as _os
    app = QApplication.instance() or QApplication([])
    lib = SqliteProjectLibrary(_os.path.join(tempfile.mkdtemp(), "lib.db"))
    w = MainWindow(library=lib)
    from tectora.application.project import Project
    w.open_editor(Project(global_pitch_deg=35.0))
    editor = w._stack.widget(1)
    return editor.canvas


def test_draw_rectangle_builds_footprint():
    c = _canvas()
    for (x, y) in [(0, 0), (12, 0), (12, 8), (0, 8)]:
        c.add_point(x, y)
    c.close_polygon()
    fp = c.footprint
    assert fp is not None and fp.n == 4
    assert all(e.type.value == "eave" for e in fp.edges)


def test_toggle_edge_to_gable():
    c = _canvas()
    for (x, y) in [(0, 0), (12, 0), (12, 8), (0, 8)]:
        c.add_point(x, y)
    c.close_polygon()
    c.toggle_edge_type(1)
    assert c.footprint.edges[1].type.value == "gable"


def test_clear_resets():
    c = _canvas()
    c.add_point(0, 0); c.add_point(1, 0); c.add_point(1, 1)
    c.close_polygon()
    c.clear()
    assert c.footprint is None


def test_footprint_none_when_open():
    c = _canvas()
    c.add_point(0, 0)
    c.add_point(5, 0)
    assert c.footprint is None


def test_close_polygon_requires_three_vertices():
    c = _canvas()
    c.add_point(0, 0)
    c.add_point(1, 0)
    try:
        c.close_polygon()
        assert False, "Should have raised ValueError"
    except ValueError:
        pass
    assert c.footprint is None


def test_move_vertex():
    c = _canvas()
    for (x, y) in [(0, 0), (10, 0), (10, 6), (0, 6)]:
        c.add_point(x, y)
    c.close_polygon()
    c.move_vertex(2, 12, 6)
    fp = c.footprint
    # Vertex 2 should be at or near (12, 6) — may be reordered by CCW normalization
    vertices = list(fp.vertices)
    assert any(abs(v[0] - 12) < 1e-9 and abs(v[1] - 6) < 1e-9 for v in vertices)


def test_set_skeleton_stores_without_error():
    c = _canvas()
    for (x, y) in [(0, 0), (12, 0), (12, 8), (0, 8)]:
        c.add_point(x, y)
    c.close_polygon()
    # G4 placeholder: set_skeleton should accept any argument and not raise
    c.set_skeleton({"edges": [], "nodes": []})
    assert True  # no exception


def test_toggle_edge_cycle():
    c = _canvas()
    for (x, y) in [(0, 0), (12, 0), (12, 8), (0, 8)]:
        c.add_point(x, y)
    c.close_polygon()
    # Initially eave
    assert c.footprint.edges[0].type.value == "eave"
    c.toggle_edge_type(0)
    assert c.footprint.edges[0].type.value == "gable"
    c.toggle_edge_type(0)
    assert c.footprint.edges[0].type.value == "eave"


def test_self_intersecting_polygon_stays_open():
    import pytest
    c = _canvas()
    # "bowtie" quad — self-intersecting
    for (x, y) in [(0, 0), (2, 2), (2, 0), (0, 2)]:
        c.add_point(x, y)
    with pytest.raises(ValueError):
        c.close_polygon()
    # Must stay open so the user can keep editing
    assert c._closed is False
    assert c.footprint is None


def test_invalid_shape_key_present_in_all_catalogs():
    from tectora.gui.i18n import tr
    for lang in ("cs", "sk", "en"):
        assert tr("invalidShape", lang) != "invalidShape"
