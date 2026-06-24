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
    return app, w, w._stack.widget(1)


def test_editor_has_menubar_with_four_menus(tmp_path):
    from PySide6.QtWidgets import QMenuBar
    app, w, e = _editor(tmp_path)
    assert isinstance(e.menubar, QMenuBar)
    titles = [m.title() for m in e.menubar.findChildren(__import__("PySide6.QtWidgets", fromlist=["QMenu"]).QMenu)]
    assert len(titles) >= 4   # Soubor / Zobrazení / Nastavení / Nápověda


def test_open_settings_method_exists(tmp_path):
    app, w, e = _editor(tmp_path)
    assert hasattr(e, "_open_settings")


def test_view_menu_switches_mode(tmp_path):
    app, w, e = _editor(tmp_path)
    e._set_view_mode("3d")
    app.processEvents()
    assert e.view_mode == "3d"
