"""Calc reports overlapping buildings: the covered part of the lower roof is
deducted from the design total (toast with the amount). Touching wall-to-wall
is not an overlap."""
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
    w.open_editor(Project(global_pitch_deg=35.0))
    return app, w, w._stack.widget(1)


def _draw(c, pts):
    for (x, y) in pts:
        c.add_point(x, y)
    c.close_polygon()


def _calc_with_captured_toasts(e):
    toasts = []
    original = e._show_toast
    e._show_toast = lambda text: toasts.append(text)
    try:
        e._on_calculate()
    finally:
        e._show_toast = original
    return toasts


def _deduction_prefix(lang="cs"):
    # "Překrytí staveb: z nižší střechy odečteno {area} m²." without the amount
    return tr("overlapDeducted", lang).split("{")[0]


def test_overlapping_buildings_report_the_deduction(tmp_path):
    app, w, e = _editor(tmp_path)
    _draw(e.canvas, [(0, 0), (10, 0), (10, 6), (0, 6)])
    e._on_new_building()
    _draw(e.canvas, [(5, 3), (12, 3), (12, 9), (5, 9)])
    toasts = _calc_with_captured_toasts(e)
    assert any(t.startswith(_deduction_prefix()) for t in toasts)


def test_adjacent_buildings_do_not_warn(tmp_path):
    app, w, e = _editor(tmp_path)
    _draw(e.canvas, [(0, 0), (10, 0), (10, 6), (0, 6)])
    e._on_new_building()
    _draw(e.canvas, [(10, 0), (16, 0), (16, 6), (10, 6)])   # wall-to-wall
    toasts = _calc_with_captured_toasts(e)
    assert tr("overlapWarning", "cs") not in toasts
    assert not any(t.startswith(_deduction_prefix()) for t in toasts)


def test_single_building_never_warns(tmp_path):
    app, w, e = _editor(tmp_path)
    _draw(e.canvas, [(0, 0), (10, 0), (10, 6), (0, 6)])
    toasts = _calc_with_captured_toasts(e)
    assert tr("overlapWarning", "cs") not in toasts
    assert not any(t.startswith(_deduction_prefix()) for t in toasts)
