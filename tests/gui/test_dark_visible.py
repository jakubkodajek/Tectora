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
    """Ensure theme is restored to light after each test so other tests are unaffected."""
    yield
    from tectora.gui import theme
    from PySide6.QtWidgets import QApplication
    theme.set_dark(False)
    app = QApplication.instance()
    if app is not None:
        theme.apply_theme(app)


def test_app_stylesheet_dark_after_toggle(tmp_path):
    from tectora.gui import theme
    app, w = _win(tmp_path)
    w.set_theme(True)
    app.processEvents()
    # the dark BG hex must appear in the applied global stylesheet
    assert theme.DARK["BG"].lower() in app.styleSheet().lower()
    w.set_theme(False)
    app.processEvents()
    assert theme.LIGHT["BG"].lower() in app.styleSheet().lower()


def test_no_hardcoded_light_panel_bg_in_dark(tmp_path):
    """After dark toggle, a freshly built editor's right-panel area should not
    carry a known light hex in its inline stylesheet."""
    from tectora.application.project import Project
    app, w = _win(tmp_path)
    w.set_theme(True)
    w.open_editor(Project(global_pitch_deg=35.0))
    app.processEvents()
    e = w._stack.widget(1)
    # the right panel container should reference the dark palette, not a light hex
    panel = getattr(e, "right_panel", None) or getattr(e, "_right_panel", None)
    if panel is not None and panel.styleSheet():
        ss = panel.styleSheet().lower()
        for light_hex in ("#ffffff", "#fafbfc", "#f0f4f8", "#f1f3f6", "#eef0f2"):
            assert light_hex not in ss, f"panel still uses {light_hex} in dark mode"
    w.set_theme(False)  # leave light for other tests
    app.processEvents()
