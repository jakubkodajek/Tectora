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


def test_overhang_control_recomputes(tmp_path):
    app, e = _editor(tmp_path)
    for (x, y) in [(0, 0), (12, 0), (12, 8), (0, 8)]:
        e.canvas.add_point(x, y)
    e.canvas.close_polygon()
    e._on_calculate()
    area0 = e.last_result.total_area_m2
    e._overhang_spin.setValue(0.5) if hasattr(e, "_overhang_spin") else e.set_overhang(0.5)
    app.processEvents()
    assert e.last_result.total_area_m2 > area0
