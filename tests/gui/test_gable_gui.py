# -*- coding: utf-8 -*-
"""GUI tests for Stage-1 gable UX in the 2D editor (T2).

Offscreen; mirrors the editor/canvas bootstrap from ``test_canvas_model.py``.
Covers: a valid wing-end edge toggles to gable and computes a roof with rake;
an invalid (long-side) edge is rejected and stays eave; the legend renders with
the new gable row.
"""
import os

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")


def _editor():
    from PySide6.QtWidgets import QApplication
    from tectora.gui.app_window import MainWindow
    from tectora.io.sqlite_library import SqliteProjectLibrary
    import tempfile
    import os as _os

    app = QApplication.instance() or QApplication([])  # noqa: F841
    lib = SqliteProjectLibrary(_os.path.join(tempfile.mkdtemp(), "lib.db"))
    w = MainWindow(library=lib)
    from tectora.application.project import Project

    w.open_editor(Project(global_pitch_deg=35.0))
    editor = w._stack.widget(1)
    # Give the canvas a real size so model->screen mapping fits the footprint.
    editor.canvas.resize(640, 480)
    return editor


# L-shaped footprint with two rectangular wing-end caps.
_L = [(0, 0), (6, 0), (6, 3), (3, 3), (3, 6), (0, 6)]


def _draw_L(canvas):
    for (x, y) in _L:
        canvas.add_point(x, y)
    canvas.close_polygon()


def _edge_screen_mid(canvas, idx):
    """Screen-space midpoint of canvas edge *idx* (draw-order vertices)."""
    n = len(canvas._vertices)
    a = canvas._to_screen(*canvas._vertices[idx])
    b = canvas._to_screen(*canvas._vertices[(idx + 1) % n])
    return (a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0


def _find_edge_by_endpoints(canvas, p, q):
    """Canvas edge index whose endpoints match {p, q}, else None."""
    n = len(canvas._vertices)
    target = {(round(p[0], 6), round(p[1], 6)), (round(q[0], 6), round(q[1], 6))}
    for i in range(n):
        a = canvas._vertices[i]
        b = canvas._vertices[(i + 1) % n]
        cur = {(round(a[0], 6), round(a[1], 6)), (round(b[0], 6), round(b[1], 6))}
        if cur == target:
            return i
    return None


def test_valid_wing_end_gable_toggles_and_computes():
    from tectora.core.gable import is_valid_gable_edge_at

    editor = _editor()
    c = editor.canvas
    _draw_L(c)
    editor.active_tool = "eaveGable"

    # Identify a valid wing-end edge by endpoint match (robust to any rewind).
    n = len(c._vertices)
    valid_idx = next(
        i
        for i in range(n)
        if is_valid_gable_edge_at(
            c.footprint, c._vertices[i], c._vertices[(i + 1) % n]
        )
    )
    # Sanity: the known cap (6,0)-(6,3) is one of the valid edges.
    assert _find_edge_by_endpoints(c, (6, 0), (6, 3)) is not None

    # Simulate the real click at that edge's screen midpoint.
    px, py = _edge_screen_mid(c, valid_idx)
    c._handle_eavegable_click(px, py)

    assert c._edge_types[valid_idx] == "gable"

    # Computing the roof must not raise and must report rake (gable) length.
    editor._on_calculate()
    assert editor.last_result is not None
    assert editor.last_result.gable_edge_total_m > 0


def test_invalid_long_side_gable_rejected():
    editor = _editor()
    c = editor.canvas
    _draw_L(c)
    editor.active_tool = "eaveGable"

    # The long side (0,0)-(6,0) is NOT a Stage-1 cap.
    idx = _find_edge_by_endpoints(c, (0, 0), (6, 0))
    assert idx is not None
    assert c._edge_types[idx] == "eave"

    # Track toast invocations so we can assert the user is informed.
    seen = []
    editor._show_toast = lambda text: seen.append(text)

    px, py = _edge_screen_mid(c, idx)
    c._handle_eavegable_click(px, py)

    # Edge must stay eave (no broken roof emitted) and a toast must have fired.
    assert c._edge_types[idx] == "eave"
    from tectora.gui.i18n import tr

    assert tr("gableNotHere", c._lang) in seen


def test_gable_to_eave_always_allowed():
    """Turning a gable back to eave skips validation (always allowed)."""
    from tectora.core.gable import is_valid_gable_edge_at

    editor = _editor()
    c = editor.canvas
    _draw_L(c)
    editor.active_tool = "eaveGable"

    n = len(c._vertices)
    valid_idx = next(
        i
        for i in range(n)
        if is_valid_gable_edge_at(
            c.footprint, c._vertices[i], c._vertices[(i + 1) % n]
        )
    )
    px, py = _edge_screen_mid(c, valid_idx)
    c._handle_eavegable_click(px, py)
    assert c._edge_types[valid_idx] == "gable"

    # Click again -> back to eave, no toast needed.
    c._handle_eavegable_click(px, py)
    assert c._edge_types[valid_idx] == "eave"


def test_legend_renders_with_gable_row():
    from PySide6.QtGui import QPixmap
    from tectora.gui.editor.canvas_2d import _LEGEND_ITEMS

    editor = _editor()
    c = editor.canvas
    _draw_L(c)

    assert ("gable", "gable") in _LEGEND_ITEMS
    assert len(_LEGEND_ITEMS) == 5

    # Render the whole canvas (incl. legend) to a pixmap — must not raise.
    pm = QPixmap(c.size())
    c.render(pm)
    assert not pm.isNull()


def test_gable_not_here_key_present_in_all_catalogs():
    from tectora.gui.i18n import tr

    for lang in ("cs", "sk", "en"):
        assert tr("gableNotHere", lang) != "gableNotHere"
