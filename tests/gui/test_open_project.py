"""Opening a saved project must restore its drawing (not show an empty canvas),
and survive a theme change."""
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


def _saved_entry(lib):
    from tectora.application import use_cases as uc
    from tectora.application.project import Project
    from tectora.core.footprint import Footprint
    fp = Footprint.from_lists([(0, 0), (6, 0), (6, 4), (0, 4)], ["eave"] * 4)
    proj = Project(project_name="Dum", footprint=fp, global_pitch_deg=35.0,
                   overhang_m=0.5, wall_height_m=2.5)
    eid = uc.save_entry(lib, proj, "Dum", note="x", result=None)
    return lib.get(eid)


@pytest.fixture(autouse=True)
def reset_theme():
    yield
    from tectora.gui import theme
    from PySide6.QtWidgets import QApplication
    theme.set_dark(False)
    app = QApplication.instance()
    if app is not None:
        theme.apply_theme(app)


def test_open_entry_restores_drawing(tmp_path):
    app, w = _win(tmp_path)
    w.open_entry(_saved_entry(w.library))
    ed = w._stack.widget(1)
    assert ed.canvas._closed is True
    assert len(ed.canvas._vertices) == 4
    assert ed.last_result is not None       # roof recomputed → results shown
    assert w._current_project is not None    # tracked on window for rebuilds


def test_open_entry_survives_theme_toggle(tmp_path):
    app, w = _win(tmp_path)
    w.open_entry(_saved_entry(w.library))
    w.set_theme(True)
    app.processEvents()
    ed = w._stack.widget(1)
    assert ed.canvas._closed is True
    assert len(ed.canvas._vertices) == 4


def test_open_new_then_clears_previous_drawing(tmp_path):
    from tectora.application.project import Project
    app, w = _win(tmp_path)
    w.open_entry(_saved_entry(w.library))
    # Opening a brand-new empty project must not keep the previous drawing.
    w.open_editor(Project(global_pitch_deg=35.0))
    ed = w._stack.widget(1)
    assert ed.canvas._closed is False
    assert ed.canvas._vertices == []
