import os

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")


def _window(tmp_path):
    from PySide6.QtWidgets import QApplication
    from tectora.gui.app_window import MainWindow
    from tectora.io.sqlite_library import SqliteProjectLibrary
    app = QApplication.instance() or QApplication([])
    lib = SqliteProjectLibrary(str(tmp_path / "lib.db"))
    return app, MainWindow(library=lib)


def test_open_editor_shows_name_and_back_returns_home(tmp_path):
    from tectora.application.project import Project
    app, w = _window(tmp_path)
    w.show(); app.processEvents()
    w.open_editor(Project(project_name="Dům X", global_pitch_deg=35.0))
    app.processEvents()
    assert w._stack.currentIndex() == 1
    editor = w._stack.widget(1)
    assert editor.view_mode in ("2d", "3d")
    w.open_home(); app.processEvents()
    assert w._stack.currentIndex() == 0
