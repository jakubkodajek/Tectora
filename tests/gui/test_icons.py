"""Painted icons: every icon must actually draw something (no empty buttons)."""
import os
os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

import pytest


def _app():
    from PySide6.QtWidgets import QApplication
    return QApplication.instance() or QApplication([])


def _has_ink(icon) -> bool:
    pm = icon.pixmap(40, 40)
    img = pm.toImage()
    return any(
        img.pixelColor(x, y).alpha() > 0
        for x in range(0, img.width(), 2)
        for y in range(0, img.height(), 2)
    )


@pytest.mark.parametrize("factory", [
    "undo_icon", "redo_icon", "rotate_icon",
    "rename_icon", "duplicate_icon", "close_icon",
])
def test_icon_draws_something(factory):
    _app()
    from tectora.gui import icons
    assert _has_ink(getattr(icons, factory)())


@pytest.mark.parametrize("direction", ["left", "right", "up", "down"])
def test_arrow_icons_draw(direction):
    _app()
    from tectora.gui.icons import arrow_icon
    assert _has_ink(arrow_icon(direction))


def test_toolbar_buttons_use_icons_not_glyph_text(tmp_path):
    from tectora.application.project import Project
    from tectora.gui.app_window import MainWindow
    from tectora.io.sqlite_library import SqliteProjectLibrary
    _app()
    lib = SqliteProjectLibrary(str(tmp_path / "lib.db"))
    w = MainWindow(library=lib)
    w.open_editor(Project(global_pitch_deg=35.0))
    e = w._stack.widget(1)
    for btn in (e._undo_btn, e._redo_btn, e._rotate_btn):
        assert btn.text() == ""            # no font-dependent glyph
        assert not btn.icon().isNull()
