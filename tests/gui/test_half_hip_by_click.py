"""Polovalba by clicking the end triangle — per edge, on any wing end.

Clicking a valba end face with the Sklon plochy tool and typing a pitch sets
a PER-EDGE half-hip depth (d = H / tan(pitch)) on that end only; the roof
recomputes via the skeleton fold, so it works on rectangles AND on the wing
ends of L/T/U shapes. The cap edge stays an eave.
"""
import math
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
    """Render (to record face hits), click the face matching *pred*."""
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


def _capture_toasts(e):
    toasts = []
    e._show_toast = lambda text: toasts.append(text)
    return toasts


def test_click_valba_clips_that_end_only(tmp_path):
    app, w, e = _editor(tmp_path)
    c = e.canvas
    _draw(c)
    e._on_calculate()                          # plain hip (all eave)

    # The left end triangle: model centroid near x = 2 + 1 = 3.
    hit = _click_face(c, lambda h: h[5] < 4.0)
    _type_pitch(c, hit, 60)
    app.processEvents()

    b = c._buildings[0]
    expected_d = 3 * math.tan(math.radians(35)) / math.tan(math.radians(60))
    carrying = [i for i, d in enumerate(b.edge_half_hips) if d > 0]
    assert len(carrying) == 1                  # ONE end clipped
    assert abs(b.edge_half_hips[carrying[0]] - expected_d) < 1e-6
    assert all(t == "eave" for t in b.edge_types)   # cap stays an eave

    e._on_calculate()
    slopes = sorted(round(f.slope_deg, 2) for f in b.skeleton_data.faces)
    assert slopes.count(60.0) == 1             # the clipped end
    assert slopes.count(35.0) == 3             # mains + the untouched end


def test_click_wing_end_on_l_shape(tmp_path):
    app, w, e = _editor(tmp_path)
    c = e.canvas
    _draw(c, L_SHAPE)
    e._on_calculate()
    plain_ridge = e.last_result.ridge_length_m

    # Wing end at x = 12 (cap edge (12,0)–(12,4), width 4): its end face
    # centroid lies right of x = 10.
    hit = _click_face(c, lambda h: h[5] > 10.0)
    _type_pitch(c, hit, 60)
    app.processEvents()
    e._on_calculate()

    b = c._buildings[0]
    h = 2.0 * math.tan(math.radians(35))       # cap width 4 → H = 2·tan(35)
    expected_d = h / math.tan(math.radians(60))
    carrying = [i for i, d in enumerate(b.edge_half_hips) if d > 0]
    assert len(carrying) == 1
    assert abs(b.edge_half_hips[carrying[0]] - expected_d) < 1e-6
    # Ridge grew by (w/2 − d) on that wing.
    assert abs(e.last_result.ridge_length_m
               - (plain_ridge + 2.0 - expected_d)) < 1e-6
    beta = math.degrees(math.atan(h / expected_d))
    assert any(abs(f.slope_deg - beta) < 0.01 for f in b.skeleton_data.faces)


def test_reclick_updates_the_same_end(tmp_path):
    app, w, e = _editor(tmp_path)
    c = e.canvas
    _draw(c)
    e._on_calculate()
    hit = _click_face(c, lambda h: h[5] < 4.0)
    _type_pitch(c, hit, 60)
    e._on_calculate()

    # The clipped end face (now a valbička plane) — click it again.
    hit2 = _click_face(c, lambda h: h[5] < 4.0)
    _type_pitch(c, hit2, 45)
    e._on_calculate()

    b = c._buildings[0]
    expected_d = 3 * math.tan(math.radians(35))     # tan(45) == 1
    carrying = [i for i, d in enumerate(b.edge_half_hips) if d > 0]
    assert len(carrying) == 1
    assert abs(b.edge_half_hips[carrying[0]] - expected_d) < 1e-6


def test_end_pitch_not_steeper_than_main_is_refused(tmp_path):
    app, w, e = _editor(tmp_path)
    c = e.canvas
    _draw(c)
    e._on_calculate()
    toasts = _capture_toasts(e)
    hit = _click_face(c, lambda h: h[5] < 4.0)
    _type_pitch(c, hit, 30)                    # main pitch is 35
    b = c._buildings[0]
    assert all(d == 0 for d in b.edge_half_hips)
    assert tr("halfHipPitchTooLow", "cs") in toasts


def test_main_face_click_still_sets_global_pitch(tmp_path):
    app, w, e = _editor(tmp_path)
    c = e.canvas
    _draw(c)
    e._on_calculate()
    # A main slope: centroid in the middle band (x ≈ 7).
    hit = _click_face(c, lambda h: 5.0 < h[5] < 9.0)
    _type_pitch(c, hit, 42)
    b = c._buildings[0]
    assert b.pitch == 42
    assert all(d == 0 for d in b.edge_half_hips)


def test_end_click_is_one_undo_step(tmp_path):
    app, w, e = _editor(tmp_path)
    c = e.canvas
    _draw(c)
    e._on_calculate()
    hit = _click_face(c, lambda h: h[5] < 4.0)
    _type_pitch(c, hit, 55)
    b = c._buildings[0]
    assert any(d > 0 for d in b.edge_half_hips)
    assert c.undo()
    assert all(d == 0 for d in c._buildings[0].edge_half_hips)


def test_legacy_symmetric_valbicka_click_updates_spin_value(tmp_path):
    app, w, e = _editor(tmp_path)
    c = e.canvas
    _draw(c)
    n = len(c._vertices)
    lens = sorted(
        (((c._vertices[(i + 1) % n][0] - c._vertices[i][0]) ** 2
          + (c._vertices[(i + 1) % n][1] - c._vertices[i][1]) ** 2), i)
        for i in range(n)
    )
    for (_l, i) in lens[:2]:
        c.toggle_edge_type(i)                  # both caps → štít
    e._half_hip_spin.setValue(1.0)             # symmetric exact-path polovalba
    e._on_calculate()

    hit = _click_face(c, lambda h: "valbička" in h[3])
    _type_pitch(c, hit, 45)
    b = c._buildings[0]
    assert abs(b.half_hip - 3 * math.tan(math.radians(35))) < 1e-6
    assert all(d == 0 for d in b.edge_half_hips)


def test_gable_marking_clears_edge_half_hip(tmp_path):
    app, w, e = _editor(tmp_path)
    c = e.canvas
    _draw(c)
    e._on_calculate()
    hit = _click_face(c, lambda h: h[5] < 4.0)
    _type_pitch(c, hit, 60)
    b = c._buildings[0]
    ei = next(i for i, d in enumerate(b.edge_half_hips) if d > 0)
    c.toggle_edge_type(ei)                     # štít replaces the polovalba
    assert b.edge_half_hips[ei] == 0.0
