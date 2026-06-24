"""G6 end-to-end GUI tests: save, open, and CRUD via the library."""
import os
os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")


def _win(tmp_path):
    from PySide6.QtWidgets import QApplication
    from tectora.gui.app_window import MainWindow
    from tectora.io.sqlite_library import SqliteProjectLibrary
    app = QApplication.instance() or QApplication([])
    lib = SqliteProjectLibrary(str(tmp_path / "lib.db"))
    return app, MainWindow(library=lib)


def _draw_rect(editor):
    for (x, y) in [(0, 0), (12, 0), (12, 8), (0, 8)]:
        editor.canvas.add_point(x, y)
    editor.canvas.close_polygon()


def test_save_new_project_appears_in_library(tmp_path):
    from tectora.application import use_cases as uc
    from tectora.application.project import Project
    app, w = _win(tmp_path)
    w.open_editor(Project(project_name="Dům X", global_pitch_deg=35.0))
    e = w._stack.widget(1)
    _draw_rect(e)
    e.name_edit.setText("Dům X")
    e.note_edit.setPlainText("pozn")
    e._on_save()                      # the Save handler
    app.processEvents()
    entries = uc.list_entries(w.library)
    assert len(entries) == 1
    assert entries[0].name == "Dům X"
    assert entries[0].result is not None


def test_open_entry_populates_fields(tmp_path):
    from tectora.application import use_cases as uc
    from tectora.application.project import Project
    from tectora.core.footprint import Footprint
    from tectora.core.location import Location
    app, w = _win(tmp_path)
    fp = Footprint.from_lists([(0, 0), (12, 0), (12, 8), (0, 8)], ["eave"] * 4)
    eid = uc.save_entry(w.library, Project(project_name="Chata", footprint=fp,
                        global_pitch_deg=30.0), "Chata", note="x",
                        location=Location("Telč", 49.18, 15.45))
    entry = uc.load_entry(w.library, eid)
    w.open_entry(entry)
    app.processEvents()
    e = w._stack.widget(1)
    assert e.get_name() == "Chata"
    assert e._current_entry_id == eid


def test_save_then_update_keeps_single_entry(tmp_path):
    from tectora.application import use_cases as uc
    from tectora.application.project import Project
    app, w = _win(tmp_path)
    w.open_editor(Project(project_name="A", global_pitch_deg=35.0))
    e = w._stack.widget(1)
    _draw_rect(e)
    e.name_edit.setText("A")
    e._on_save()
    e.name_edit.setText("A2")
    e._on_save()
    app.processEvents()
    entries = uc.list_entries(w.library)
    assert len(entries) == 1
    assert entries[0].name == "A2"
