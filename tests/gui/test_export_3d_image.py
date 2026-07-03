"""Export the 3D scene as a PNG (Soubor → Export 3D obrázku…)."""
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
    w.open_editor(Project(project_name="Test", global_pitch_deg=35.0))
    return app, w, w._stack.widget(1)


RECT = [(0, 0), (10, 0), (10, 6), (0, 6)]


def test_export_writes_a_valid_png(tmp_path):
    from PySide6.QtGui import QPixmap
    app, w, e = _editor(tmp_path)
    for (x, y) in RECT:
        e.canvas.add_point(x, y)
    e.canvas.close_polygon()

    path = str(tmp_path / "strecha.png")
    assert e.export_3d_image(path)
    assert os.path.exists(path)
    pm = QPixmap(path)
    assert not pm.isNull()
    assert pm.width() == 1200 and pm.height() == 800


def test_export_without_drawing_refuses(tmp_path):
    app, w, e = _editor(tmp_path)
    path = str(tmp_path / "empty.png")
    assert not e.export_3d_image(path)
    assert not os.path.exists(path)
