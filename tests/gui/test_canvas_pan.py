"""Right-button panning + live edge-length rendering on the 2D canvas."""
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
    editor.canvas.resize(600, 400)
    return editor.canvas


def _mouse(kind, x, y, button, buttons):
    from PySide6.QtCore import QPointF, Qt
    from PySide6.QtGui import QMouseEvent
    types = {
        "press": QMouseEvent.Type.MouseButtonPress,
        "move": QMouseEvent.Type.MouseMove,
        "release": QMouseEvent.Type.MouseButtonRelease,
    }
    return QMouseEvent(types[kind], QPointF(x, y), button, buttons,
                       Qt.KeyboardModifier.NoModifier)


def test_origin_includes_pan():
    c = _canvas()
    cx, cy = c._origin_px()
    c._pan = (40.0, 30.0)
    nx, ny = c._origin_px()
    assert (nx - cx, ny - cy) == (40.0, 30.0)


def test_right_drag_pans_view():
    from PySide6.QtCore import Qt
    c = _canvas()
    c._pan = (0.0, 0.0)
    c.mousePressEvent(_mouse("press", 100, 100, Qt.MouseButton.RightButton,
                             Qt.MouseButton.RightButton))
    c.mouseMoveEvent(_mouse("move", 135, 120, Qt.MouseButton.NoButton,
                            Qt.MouseButton.RightButton))
    assert c._pan == (35.0, 20.0)
    c.mouseReleaseEvent(_mouse("release", 135, 120, Qt.MouseButton.RightButton,
                               Qt.MouseButton.NoButton))
    assert c._pan_anchor is None


def test_clear_resets_pan():
    c = _canvas()
    c._pan = (50.0, -10.0)
    c.clear()
    assert c._pan == (0.0, 0.0)


def test_paint_while_drawing_does_not_crash():
    # Exercises _draw_in_progress + the live length labels (_draw_seg_len_label).
    from PySide6.QtGui import QPixmap
    c = _canvas()
    c.add_point(0, 0)
    c.add_point(4, 0)
    c._snap_pt = (4, 3)            # simulate a hovered cursor for the preview seg
    pm = QPixmap(c.size())
    c.render(pm)                    # must not raise
