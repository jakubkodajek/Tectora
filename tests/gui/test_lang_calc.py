import os
os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")


def _win(tmp_path):
    from PySide6.QtWidgets import QApplication
    from tectora.gui.app_window import MainWindow
    from tectora.io.sqlite_library import SqliteProjectLibrary
    app = QApplication.instance() or QApplication([])
    lib = SqliteProjectLibrary(str(tmp_path / "lib.db"))
    return app, MainWindow(library=lib)


def test_calc_works_after_language_switch(tmp_path):
    from tectora.application.project import Project
    app, w = _win(tmp_path)
    w.open_editor(Project(global_pitch_deg=35.0))
    e = w._stack.widget(1)
    for (x, y) in [(0, 0), (12, 0), (12, 8), (0, 8)]:
        e.canvas.add_point(x, y)
    e.canvas.close_polygon()
    w.set_language("en")
    app.processEvents()
    e2 = w._stack.widget(1)               # rebuilt editor
    assert e2.canvas.footprint is not None  # edge_types restored
    e2._on_calculate()
    assert e2.last_result is not None
