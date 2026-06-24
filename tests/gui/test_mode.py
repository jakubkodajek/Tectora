"""FX-B: appearance-mode selector + primary-button theming tests."""
import os
os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

import pytest


def _win(tmp_path):
    from PySide6.QtWidgets import QApplication
    from tectora.gui.app_window import MainWindow
    from tectora.io.sqlite_library import SqliteProjectLibrary
    app = QApplication.instance() or QApplication([])
    lib = SqliteProjectLibrary(str(tmp_path / "lib.db"))
    return app, MainWindow(library=lib)


@pytest.fixture(autouse=True)
def reset_theme():
    """Restore the light theme after each test so other tests are unaffected."""
    yield
    from tectora.gui import theme
    from PySide6.QtWidgets import QApplication
    theme.set_dark(False)
    app = QApplication.instance()
    if app is not None:
        theme.apply_theme(app)


# ---------------------------------------------------------------------------
# MainWindow.set_mode
# ---------------------------------------------------------------------------

def test_set_mode_dark(tmp_path):
    app, w = _win(tmp_path)
    w.set_mode("dark")
    app.processEvents()
    assert w.dark is True
    assert w.mode == "dark"


def test_set_mode_light(tmp_path):
    app, w = _win(tmp_path)
    w.set_mode("dark")
    app.processEvents()
    w.set_mode("light")
    app.processEvents()
    assert w.dark is False
    assert w.mode == "light"


def test_set_mode_system_does_not_raise(tmp_path):
    app, w = _win(tmp_path)
    w.set_mode("system")  # must not raise on offscreen
    app.processEvents()
    assert w.mode == "system"
    assert isinstance(w.dark, bool)


def test_set_mode_ignores_unknown(tmp_path):
    app, w = _win(tmp_path)
    w.set_mode("bogus")
    assert w.mode == "light"


# ---------------------------------------------------------------------------
# SettingsDialog mode selector + primary OK button
# ---------------------------------------------------------------------------

def test_settings_dialog_preselects_mode(tmp_path):
    from tectora.gui.settings_dialog import SettingsDialog
    app, w = _win(tmp_path)
    d = SettingsDialog(current_lang="cs", current_mode="dark")
    assert d.selected_mode() == "dark"


def test_settings_dialog_ok_button_is_primary(tmp_path):
    from tectora.gui.settings_dialog import SettingsDialog
    app, w = _win(tmp_path)
    d = SettingsDialog(current_lang="cs", current_mode="light")
    assert d._ok_btn.property("primary") is True


# ---------------------------------------------------------------------------
# Theme QSS sanity — primary rule present, white text literal
# ---------------------------------------------------------------------------

def test_primary_rule_present_in_qss():
    from tectora.gui import theme
    theme.set_dark(True)  # rebuilds GLOBAL_QSS for the dark palette
    assert 'primary="true"' in theme.GLOBAL_QSS
    # the primary block keeps a literal white text colour (readable in dark mode)
    primary_idx = theme.GLOBAL_QSS.index('QPushButton[primary="true"]')
    primary_block = theme.GLOBAL_QSS[primary_idx:primary_idx + 200]
    assert "#ffffff" in primary_block
