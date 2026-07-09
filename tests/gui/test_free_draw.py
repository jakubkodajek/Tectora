"""Free outline drawing: park chains with Esc, continue from loose ends,
join two chains with a segment, and close when the ends meet.

The footprint no longer has to be drawn vertex-by-vertex around the outline.
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
    w.open_editor(Project(global_pitch_deg=35.0))
    ed = w._stack.widget(1)
    ed.canvas.resize(800, 500)
    return app, w, ed


def _press_escape(widget):
    from PySide6.QtCore import Qt, QEvent
    from PySide6.QtGui import QKeyEvent
    from PySide6.QtWidgets import QApplication
    ev = QKeyEvent(QEvent.Type.KeyPress, Qt.Key.Key_Escape,
                   Qt.KeyboardModifier.NoModifier)
    QApplication.sendEvent(widget, ev)


def _click_model(c, mx, my):
    px, py = c._to_screen(mx, my)
    c._handle_draw_click(px, py)


def test_continue_from_a_parked_chain_end(tmp_path):
    app, w, ed = _editor(tmp_path)
    c = ed.canvas
    c.add_point(0, 0)
    c.add_point(5, 0)
    _press_escape(c)                       # park the chain
    assert c._vertices == []
    _click_model(c, 5, 0)                  # pick it up at its tail
    assert c._buildings[0].extra_chains == []
    assert c._vertices == [(0.0, 0.0), (5.0, 0.0)]
    _click_model(c, 5, 3)                  # and keep drawing
    assert c._vertices == [(0.0, 0.0), (5.0, 0.0), (5.0, 3.0)]


def test_continue_from_chain_head_reverses_it(tmp_path):
    app, w, ed = _editor(tmp_path)
    c = ed.canvas
    c.add_point(0, 0)
    c.add_point(5, 0)
    _press_escape(c)
    _click_model(c, 0, 0)                  # pick it up at its HEAD
    assert c._vertices == [(5.0, 0.0), (0.0, 0.0)]


def test_join_two_chains_and_close_the_rectangle(tmp_path):
    app, w, ed = _editor(tmp_path)
    c = ed.canvas
    # Bottom edge …
    c.add_point(0, 0)
    c.add_point(5, 0)
    _press_escape(c)
    # … top edge drawn separately (opposite direction) …
    c.add_point(5, 3)
    c.add_point(0, 3)
    _press_escape(c)
    assert len(c._buildings[0].extra_chains) == 2

    _click_model(c, 5, 0)                  # continue the bottom chain
    _click_model(c, 5, 3)                  # JOIN: segment (5,0)–(5,3)
    assert c._buildings[0].extra_chains == []
    assert c._vertices == [(0.0, 0.0), (5.0, 0.0), (5.0, 3.0), (0.0, 3.0)]
    assert not c._closed

    _click_model(c, 0, 0)                  # click the start → closes
    assert c._closed
    assert c.footprint is not None and c.footprint.n == 4


def test_joining_the_last_two_ends_closes_automatically(tmp_path):
    """Connecting the final pair of loose ends closes the outline without a
    separate 'click the start' step."""
    app, w, ed = _editor(tmp_path)
    c = ed.canvas
    # Left + bottom + right drawn as one chain, ends (0,3) and (5,3) …
    for pt in [(0, 3), (0, 0), (5, 0), (5, 3)]:
        c.add_point(*pt)
    _press_escape(c)
    # … the top edge separately, ends (0,3) and (5,3).
    c.add_point(0, 3)
    c.add_point(5, 3)

    # The active (top) chain ends at (5,3); clicking the big chain's loose
    # end at the SAME spot joins them there — and the outline's remaining
    # ends (0,3)/(0,3) coincide too, so the polygon closes automatically.
    _click_model(c, 5, 3)
    assert c._closed
    assert c._buildings[0].extra_chains == []
    assert c.footprint is not None and c.footprint.n == 4


def test_close_with_leftover_chain_toasts(tmp_path):
    app, w, ed = _editor(tmp_path)
    c = ed.canvas
    toasts = []
    ed._show_toast = lambda t: toasts.append(t)
    # A stray parked segment elsewhere…
    c.add_point(20, 20)
    c.add_point(25, 20)
    _press_escape(c)
    # …and a triangle drawn normally.
    for pt in [(0, 0), (5, 0), (5, 3)]:
        c.add_point(*pt)
    _click_model(c, 0, 0)                  # try to close
    assert not c._closed
    assert tr("openChainsLeft", "cs") in toasts


def test_undo_restores_the_join(tmp_path):
    app, w, ed = _editor(tmp_path)
    c = ed.canvas
    c.add_point(0, 0)
    c.add_point(5, 0)
    _press_escape(c)
    c.add_point(5, 3)
    c.add_point(0, 3)
    _press_escape(c)
    _click_model(c, 5, 0)
    _click_model(c, 5, 3)                  # join
    assert c._buildings[0].extra_chains == []
    assert c.undo()                        # un-join
    assert len(c._buildings[0].extra_chains) == 1
    assert c._vertices == [(0.0, 0.0), (5.0, 0.0)]
