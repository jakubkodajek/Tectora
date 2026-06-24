"""F2: Settings dialog — language + dark theme tests."""
import os
os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")


def _win(tmp_path):
    from PySide6.QtWidgets import QApplication
    from tectora.gui.app_window import MainWindow
    from tectora.io.sqlite_library import SqliteProjectLibrary
    app = QApplication.instance() or QApplication([])
    lib = SqliteProjectLibrary(str(tmp_path / "lib.db"))
    return app, MainWindow(library=lib)


def test_set_dark_changes_palette():
    from tectora.gui import theme
    theme.set_dark(True)
    assert theme.is_dark() is True
    dark_bg = theme.BG
    theme.set_dark(False)
    assert theme.BG != dark_bg
    assert theme.is_dark() is False


def test_set_theme_on_window(tmp_path):
    app, w = _win(tmp_path)
    w.set_theme(True)
    app.processEvents()
    assert w.dark is True
    assert w._stack.widget(0) is not None
    w.set_theme(False)
    app.processEvents()
    assert w.dark is False


def test_settings_dialog_reports_choices(tmp_path):
    from tectora.gui.settings_dialog import SettingsDialog
    app, w = _win(tmp_path)
    d = SettingsDialog(current_lang="cs", current_dark=False)
    # simulate user choosing EN + dark (use the dialog's setters/widgets)
    d.set_choices("en", True)
    assert d.selected_lang() == "en"
    assert d.selected_dark() is True
