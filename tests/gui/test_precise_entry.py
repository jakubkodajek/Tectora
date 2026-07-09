"""Precise drawing: length + direction arrows, and clicking an edge-length
label to rewrite it."""
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
    return app, w, ed


def _paint(c):
    from PySide6.QtGui import QPixmap
    pm = QPixmap(c.size())
    c.render(pm)


# ── Feature A: length + direction ──────────────────────────────────────────

def test_length_and_arrows_draw_exact_rectangle(tmp_path):
    app, w, ed = _editor(tmp_path)
    c = ed.canvas
    ed._len_input.setText("5"); ed._add_relative(1, 0)    # (0,0)->(5,0)
    ed._len_input.setText("3"); ed._add_relative(0, 1)    # ->(5,3)
    ed._len_input.setText("5"); ed._add_relative(-1, 0)   # ->(0,3)
    assert c._vertices == [(0.0, 0.0), (5.0, 0.0), (5.0, 3.0), (0.0, 3.0)]
    c.close_polygon()
    assert c.footprint.n == 4
    assert round(c.footprint.edge_length(0), 6) == 5.0


def test_invalid_length_is_ignored(tmp_path):
    app, w, ed = _editor(tmp_path)
    c = ed.canvas
    ed._len_input.setText("abc"); ed._add_relative(1, 0)
    assert c._vertices == []
    ed._len_input.setText("-2"); ed._add_relative(1, 0)
    assert c._vertices == []


# ── Feature B: edit an edge length ─────────────────────────────────────────

def test_edit_edge_length_resizes_edge(tmp_path):
    app, w, ed = _editor(tmp_path)
    c = ed.canvas
    for x, y in [(0, 0), (5, 0), (5, 3), (0, 3)]:
        c.add_point(x, y)
    c.close_polygon()
    ed._on_calculate()
    _paint(c)
    assert len(c._edge_len_hits) == 4
    # Edge 0 = (0,0)-(5,0); resize to 8.
    c.set_edge_length(0, 0, 8.0)
    assert c._vertices[1] == (8.0, 0.0)
    assert ed.last_result is not None      # roof recomputed live


def test_edit_edge_length_grows_up_or_right(tmp_path):
    """Rewriting a length extends the edge upward / to the right, no matter
    which direction the edge was drawn in."""
    app, w, ed = _editor(tmp_path)
    c = ed.canvas
    for x, y in [(0, 0), (5, 0), (5, 3), (0, 3)]:
        c.add_point(x, y)
    c.close_polygon()
    # Edge 2 = (5,3)->(0,3) points LEFT; the RIGHT endpoint must move.
    c.set_edge_length(0, 2, 8.0)
    assert c._vertices[2] == (8.0, 3.0)
    assert c._vertices[3] == (0.0, 3.0)
    # Edge 3 = (0,3)->(0,0) points DOWN; the TOP endpoint must move.
    c.set_edge_length(0, 3, 8.0)
    assert c._vertices[3] == (0.0, 8.0)
    assert c._vertices[0] == (0.0, 0.0)


def test_edit_edge_length_inline_editor(tmp_path):
    app, w, ed = _editor(tmp_path)
    c = ed.canvas
    for x, y in [(0, 0), (5, 0), (5, 3), (0, 3)]:
        c.add_point(x, y)
    c.close_polygon()
    ed._on_calculate()
    _paint(c)
    hit = next(h for h in c._edge_len_hits if h[3] == 0)
    c._open_len_editor(hit[2], hit[3], hit[4], hit[0], hit[1])
    assert c._len_editor is not None
    c._len_editor.setText("6")
    c._apply_len_editor(hit[2], hit[3])
    assert c._vertices[1][0] == 6.0
    assert c._len_editor is None            # editor closed after apply


def test_length_label_hit_test(tmp_path):
    app, w, ed = _editor(tmp_path)
    c = ed.canvas
    for x, y in [(0, 0), (5, 0), (5, 3), (0, 3)]:
        c.add_point(x, y)
    c.close_polygon()
    ed._on_calculate()
    _paint(c)
    lx, ly, bi, ei, length = c._edge_len_hits[0]
    assert c._hit_edge_len_label(lx, ly) is not None
    assert c._hit_edge_len_label(lx + 200, ly + 200) is None
