"""Undo/redo on the canvas + restoring all buildings when reopening a design."""
import os
os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")


def _editor(tmp_path):
    from PySide6.QtWidgets import QApplication
    from tectora.gui.app_window import MainWindow
    from tectora.io.sqlite_library import SqliteProjectLibrary
    from tectora.application.project import Project
    app = QApplication.instance() or QApplication([])
    lib = SqliteProjectLibrary(str(tmp_path / "lib.db"))
    w = MainWindow(library=lib)
    w.open_editor(Project(global_pitch_deg=35.0))
    ed = w._stack.widget(1)
    ed.canvas.resize(800, 500)
    return app, w, ed


def _rect(c, pts):
    for x, y in pts:
        c.add_point(x, y)
    c.close_polygon()


def test_undo_redo_points(tmp_path):
    app, w, ed = _editor(tmp_path)
    c = ed.canvas
    c.add_point(0, 0)
    c.add_point(6, 0)
    c.add_point(6, 4)
    assert len(c._vertices) == 3
    ed._on_undo()
    assert len(c._vertices) == 2
    ed._on_undo()
    assert len(c._vertices) == 1
    ed._on_redo()
    assert len(c._vertices) == 2


def test_undo_close_polygon(tmp_path):
    app, w, ed = _editor(tmp_path)
    c = ed.canvas
    _rect(c, [(0, 0), (6, 0), (6, 4), (0, 4)])
    assert c._closed is True
    ed._on_undo()
    assert c._closed is False
    ed._on_redo()
    assert c._closed is True


def test_undo_new_building(tmp_path):
    app, w, ed = _editor(tmp_path)
    c = ed.canvas
    _rect(c, [(0, 0), (6, 0), (6, 4), (0, 4)])
    ed._on_new_building()
    assert c.building_count() == 2
    ed._on_undo()
    assert c.building_count() == 1
    ed._on_redo()
    assert c.building_count() == 2


def test_undo_toggle_edge(tmp_path):
    app, w, ed = _editor(tmp_path)
    c = ed.canvas
    _rect(c, [(0, 0), (8, 0), (8, 5), (0, 5)])
    # Gable the short cap edge (valid), then undo.
    from tectora.core.gable import is_valid_gable_edge_at
    n = len(c._vertices)
    gi = next(i for i in range(n)
              if is_valid_gable_edge_at(c.footprint, c._vertices[i], c._vertices[(i + 1) % n]))
    c.toggle_edge_type(gi)
    assert c._edge_types[gi] == "gable"
    ed._on_undo()
    assert c._edge_types[gi] == "eave"


def test_undo_redo_empty_is_noop(tmp_path):
    app, w, ed = _editor(tmp_path)
    assert ed.canvas.undo() is False
    assert ed.canvas.redo() is False


def test_history_reset_on_load(tmp_path):
    app, w, ed = _editor(tmp_path)
    _rect(ed.canvas, [(0, 0), (6, 0), (6, 4), (0, 4)])
    assert ed.canvas.can_undo() is True
    # Saving + reopening should not carry stale undo history.
    ed.name_edit.setText("X")
    ed._on_save()
    eid = w.library.list()[0].id
    w.open_entry(w.library.get(eid))
    ed2 = w._stack.widget(1)
    assert ed2.canvas.can_undo() is False


def test_reopen_restores_all_buildings(tmp_path):
    app, w, ed = _editor(tmp_path)
    c = ed.canvas
    _rect(c, [(0, 0), (6, 0), (6, 4), (0, 4)])
    ed._overhang_spin.setValue(0.0)
    ed._on_new_building()
    _rect(c, [(12, 0), (18, 0), (18, 4), (12, 4)])
    ed._overhang_spin.setValue(0.7)
    ed.name_edit.setText("Dvůr")
    ed._on_save()
    eid = w.library.list()[0].id

    w.open_entry(w.library.get(eid))
    ed2 = w._stack.widget(1)
    assert ed2.canvas.building_count() == 2
    assert ed2.canvas._buildings[1].overhang == 0.7
