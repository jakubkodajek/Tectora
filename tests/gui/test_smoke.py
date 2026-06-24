# -*- coding: utf-8 -*-
"""GUI smoke tests: full workflow without exceptions (G9)."""
import os

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")


def _win(tmp_path):
    from PySide6.QtWidgets import QApplication
    from tectora.gui.app_window import MainWindow
    from tectora.io.sqlite_library import SqliteProjectLibrary
    app = QApplication.instance() or QApplication([])
    lib = SqliteProjectLibrary(str(tmp_path / "lib.db"))
    return app, MainWindow(library=lib)


def test_full_smoke(tmp_path):
    from tectora.application.project import Project
    app, w = _win(tmp_path)
    w.show(); app.processEvents()
    w.open_editor(Project(project_name="Smoke", global_pitch_deg=35.0))
    e = w._stack.widget(1)
    for (x, y) in [(0, 0), (10, 0), (10, 6), (0, 6)]:
        e.canvas.add_point(x, y)
    e.canvas.close_polygon()
    e._on_calculate()
    app.processEvents()
    assert e.last_result is not None
    out = str(tmp_path / "report.xlsx")
    e._export_to(out)
    import os as _os
    assert _os.path.exists(out)
    w.set_language("en"); app.processEvents()
    w.open_home(); app.processEvents()


def test_export_xlsx_has_vykaz_sheet(tmp_path):
    import openpyxl
    from tectora.application.project import Project
    app, w = _win(tmp_path)
    w.open_editor(Project(project_name="X", global_pitch_deg=35.0))
    e = w._stack.widget(1)
    for (x, y) in [(0, 0), (12, 0), (12, 8), (0, 8)]:
        e.canvas.add_point(x, y)
    e.canvas.close_polygon()
    e._on_calculate()
    out = str(tmp_path / "r.xlsx")
    e._export_to(out)
    assert "Výkaz" in openpyxl.load_workbook(out).sheetnames
