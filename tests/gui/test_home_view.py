import os

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")


def _seed(library, n):
    from tectora.application import use_cases as uc
    from tectora.application.project import Project
    from tectora.core.footprint import Footprint
    for i in range(n):
        fp = Footprint.from_lists([(0, 0), (12, 0), (12, 8), (0, 8)],
                                  ["eave", "gable", "eave", "gable"])
        uc.save_entry(library, Project(project_name=f"Dům {i}", footprint=fp,
                                       global_pitch_deg=35.0),
                      f"Dům {i}", note="", location=None,
                      result={"results": {"total_area_m2": 100.0 + i}})


def _window(tmp_path, n):
    from PySide6.QtWidgets import QApplication
    from tectora.gui.app_window import MainWindow
    from tectora.io.sqlite_library import SqliteProjectLibrary
    app = QApplication.instance() or QApplication([])
    lib = SqliteProjectLibrary(str(tmp_path / "lib.db"))
    _seed(lib, n)
    return app, MainWindow(library=lib)


def test_home_shows_one_card_per_entry(tmp_path):
    app, w = _window(tmp_path, 3)
    w.show(); app.processEvents()
    home = w._stack.widget(0)
    assert len(home.cards) == 3


def test_home_empty_state(tmp_path):
    app, w = _window(tmp_path, 0)
    w.show(); app.processEvents()
    home = w._stack.widget(0)
    assert len(home.cards) == 0
