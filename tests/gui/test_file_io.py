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
    w.open_editor(Project(project_name="Dům", global_pitch_deg=35.0))
    return app, w._stack.widget(1)


def test_save_then_open_file_roundtrip(tmp_path):
    app, e = _editor(tmp_path)
    for (x, y) in [(0, 0), (12, 0), (12, 8), (0, 8)]:
        e.canvas.add_point(x, y)
    e.canvas.close_polygon()
    e.name_edit.setText("Chata")
    path = str(tmp_path / "share.tectora")
    e._save_to_file(path)
    assert os.path.exists(path)
    # open into a fresh editor
    app2, e2 = _editor(tmp_path)
    e2._open_from_file(path)
    app.processEvents()
    assert e2.get_name() == "Chata"
    assert e2.canvas.footprint is not None
    assert e2.last_result is not None
