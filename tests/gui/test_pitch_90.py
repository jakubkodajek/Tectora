"""Typing 90° into the face-pitch editor turns the face into a wall.

* END face (valba / valbička) → that end becomes a vertical štít.
* MAIN face → the whole roof converts to a pult (shed) draining to the
  opposite eave; the other slope keeps its angle.
"""
import os
os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

from tectora.gui.i18n import tr


def _editor(tmp_path):
    from PySide6.QtWidgets import QApplication
    from tectora.application.project import Project
    from tectora.gui.app_window import MainWindow
    from tectora.io.sqlite_library import SqliteProjectLibrary
    app = QApplication.instance() or QApplication([])
    lib = SqliteProjectLibrary(str(tmp_path / "lib.db"))
    w = MainWindow(library=lib)
    w.open_editor(Project(project_name="Test", global_pitch_deg=35.0))
    e = w._stack.widget(1)
    e.canvas.resize(700, 460)
    return app, w, e


RECT = [(2, 2), (12, 2), (12, 8), (2, 8)]     # 10 × 6, off-origin
L_SHAPE = [(0, 0), (12, 0), (12, 4), (4, 4), (4, 10), (0, 10)]


def _draw(c, pts=RECT):
    for (x, y) in pts:
        c.add_point(x, y)
    c.close_polygon()


def _click_face(c, pred):
    from PySide6.QtGui import QPixmap
    pm = QPixmap(c.size())
    c.render(pm)
    hit = next(h for h in c._face_hits if pred(h))
    c._handle_face_pitch_click(hit[0], hit[1])
    assert c._pitch_editor is not None
    return hit


def _type_pitch(c, hit, value):
    c._pitch_editor.setText(str(value))
    c._apply_pitch_editor(hit[3])


def test_90_on_end_face_makes_a_gable(tmp_path):
    app, w, e = _editor(tmp_path)
    c = e.canvas
    _draw(c)
    e._on_calculate()                          # plain hip (all eave)
    # Left end triangle (centroid near x = 3) → 90° → vertical štít.
    hit = _click_face(c, lambda h: h[5] < 4.0)
    _type_pitch(c, hit, 90)
    app.processEvents()

    b = c._buildings[0]
    assert b.edge_types.count("gable") == 1
    assert all(d == 0.0 for d in b.edge_half_hips)
    e._on_calculate()
    assert c._skeleton_data is not None
    # The fold turned the end arcs into rakes → "gable" segments exist.
    kinds = [s[0] for s in c._skeleton_data.skeleton_segments]
    assert "gable" in kinds


def test_90_on_valbicka_keeps_the_other_end(tmp_path):
    """Legacy symmetric polovalba: 90° on one valbička leaves the other end's
    clip in place (migrated to a per-edge depth)."""
    app, w, e = _editor(tmp_path)
    c = e.canvas
    _draw(c)
    # Both short ends gabled + symmetric polovalba 1.0 m (exact HALF_HIP).
    for i, (a, bpt) in enumerate(zip(RECT, RECT[1:] + RECT[:1])):
        if abs(a[0] - bpt[0]) < 1e-9:          # vertical (short) edges
            c.toggle_edge_type(i)
    e._half_hip_spin.setValue(1.0)
    e._on_calculate()
    b = c._buildings[0]
    assert b.half_hip == 1.0

    # Right valbička (centroid x > 10) → 90° → pure štít on the right.
    hit = _click_face(c, lambda h: h[5] > 10.0 and "valb" in h[3])
    _type_pitch(c, hit, 90)
    app.processEvents()

    right = next(i for i, (a, bpt) in enumerate(zip(RECT, RECT[1:] + RECT[:1]))
                 if abs(a[0] - 12) < 1e-9 and abs(bpt[0] - 12) < 1e-9)
    left = (right + 2) % 4
    assert b.edge_types[right] == "gable"
    assert b.edge_half_hips[right] == 0.0
    assert b.half_hip == 0.0
    assert b.edge_types[left] == "eave"        # valbička preserved per-edge
    assert abs(b.edge_half_hips[left] - 1.0) < 1e-9
    e._on_calculate()
    assert c._skeleton_data is not None


def test_90_on_main_face_converts_to_shed(tmp_path):
    app, w, e = _editor(tmp_path)
    c = e.canvas
    _draw(c)
    # Sedlová with an asymmetric back slope.
    for i, (a, bpt) in enumerate(zip(RECT, RECT[1:] + RECT[:1])):
        if abs(a[0] - bpt[0]) < 1e-9:
            c.toggle_edge_type(i)
    e._pitch_back_spin.setValue(20)
    e._on_calculate()

    toasts = []
    e._show_toast = lambda t: toasts.append(t)
    # Click the FRONT plane (name contains "přední") and type 90.
    hit = _click_face(c, lambda h: "přední" in h[3])
    _type_pitch(c, hit, 90)
    app.processEvents()

    b = c._buildings[0]
    assert b.edge_types.count("eave") == 1     # single low side
    assert b.pitch == 20                       # the back slope's angle remains
    assert b.pitch_back is None
    assert tr("becameShed", "cs") in toasts
    e._on_calculate()
    assert c._skeleton_data.spec.type.value == "shed"
    # The low (z = 0) edge of the shed is the edge OPPOSITE the clicked front
    # face — the front face was at min-y, so the eave is at max-y (y = 8).
    face = c._skeleton_data.faces[0]
    lows = [v for v in face.vertices if abs(v[2]) < 1e-9]
    assert all(abs(v[1] - 8.0) < 1e-6 for v in lows)


def test_90_on_main_face_of_l_shape_toasts(tmp_path):
    app, w, e = _editor(tmp_path)
    c = e.canvas
    _draw(c, L_SHAPE)
    e._on_calculate()
    toasts = []
    e._show_toast = lambda t: toasts.append(t)
    types_before = list(c._buildings[0].edge_types)
    # A main slope of the long wing (not a cap face): centroid near (6, 1).
    hit = _click_face(c, lambda h: h[5] < 10.0 and h[6] < 2.5)
    _type_pitch(c, hit, 90)
    app.processEvents()
    assert c._buildings[0].edge_types == types_before
    assert tr("shedOnlyRect", "cs") in toasts
