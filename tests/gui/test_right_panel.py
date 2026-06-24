import os
os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")


class FakeGeocoder:
    def search(self, query, limit=5):
        from tectora.core.location import Location
        return [Location("Brno", 49.2, 16.6), Location("Brno-střed", 49.19, 16.6)]


def _editor(tmp_path):
    from PySide6.QtWidgets import QApplication
    from tectora.application.project import Project
    from tectora.gui.app_window import MainWindow
    from tectora.io.sqlite_library import SqliteProjectLibrary
    app = QApplication.instance() or QApplication([])
    lib = SqliteProjectLibrary(str(tmp_path / "lib.db"))
    w = MainWindow(library=lib)
    w.open_editor(Project(project_name="Dům", global_pitch_deg=35.0))
    return app, w._stack.widget(1)


def test_location_search_returns_results():
    from PySide6.QtWidgets import QApplication
    from tectora.gui.editor.location_field import LocationField
    _app = QApplication.instance() or QApplication([])
    f = LocationField(geocoder=FakeGeocoder())
    results = f._run_search("Br")
    assert len(results) == 2
    assert results[0].name == "Brno"


def test_location_select_sets_location():
    from PySide6.QtWidgets import QApplication
    from tectora.core.location import Location
    from tectora.gui.editor.location_field import LocationField
    _app = QApplication.instance() or QApplication([])
    f = LocationField(geocoder=FakeGeocoder())
    f.set_selected(Location("Brno", 49.2, 16.6))
    assert f.location is not None and f.location.name == "Brno"


def test_pitch_control_updates_editor(tmp_path):
    app, e = _editor(tmp_path)
    e.set_pitch(42)
    app.processEvents()
    assert e.pitch == 42


def test_get_name_reflects_field(tmp_path):
    app, e = _editor(tmp_path)
    e.name_edit.setText("Chata")
    assert e.get_name() == "Chata"
