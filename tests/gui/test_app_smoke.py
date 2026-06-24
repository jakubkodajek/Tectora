import os

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")


def test_mainwindow_builds_and_navigates_offscreen(tmp_path):
    from PySide6.QtWidgets import QApplication

    from tectora.gui.app_window import MainWindow
    from tectora.io.sqlite_library import SqliteProjectLibrary

    app = QApplication.instance() or QApplication([])
    library = SqliteProjectLibrary(str(tmp_path / "lib.db"))
    window = MainWindow(library=library)
    window.show()
    app.processEvents()

    window.open_editor(project=None)
    app.processEvents()
    window.open_home()
    app.processEvents()

    assert window is not None
