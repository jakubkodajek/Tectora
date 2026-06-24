import os
os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")


def _editor(tmp_path):
    from PySide6.QtWidgets import QApplication
    from tectora.application.project import Project
    from tectora.gui.app_window import MainWindow
    from tectora.io.sqlite_library import SqliteProjectLibrary
    app = QApplication.instance() or QApplication([])
    lib = SqliteProjectLibrary(str(tmp_path / "lib.db"))
    w = MainWindow(library=lib)
    w.open_editor(Project(global_pitch_deg=35.0))
    return app, w._stack.widget(1)


def test_pitch_minus_plus(tmp_path):
    app, e = _editor(tmp_path)
    e._pitch_minus()
    assert e.pitch == 34
    e._pitch_plus()
    e._pitch_plus()
    assert e.pitch == 36


def test_pitch_clamped(tmp_path):
    app, e = _editor(tmp_path)
    e.set_pitch(89)
    e._pitch_plus()
    assert e.pitch == 89  # clamped at 89


def test_canvas_zoom(tmp_path):
    app, e = _editor(tmp_path)
    s0 = e.canvas._scale
    e.canvas.zoom_in()
    assert e.canvas._scale > s0
    e.canvas.zoom_out()
    e.canvas.zoom_out()
    assert e.canvas._scale < s0
