"""Design report includes absolute eave/ridge heights above ground."""
import os
os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

from openpyxl import load_workbook

from tectora.application.use_cases import DesignResult
from tectora.core.footprint import Footprint
from tectora.core.roof_generator import generate_roof
from tectora.io.excel import write_design_report_xlsx

RECT = [(0, 0), (10, 0), (10, 6), (0, 6)]


def _rows(path):
    ws = load_workbook(path)["Výkaz"]
    return {row[0].value: row[1].value
            for row in ws.iter_rows(min_col=1, max_col=2)
            if row[0].value is not None}


def test_heights_rows_present_with_eaves(tmp_path):
    fp = Footprint.from_lists(RECT, ["eave"] * 4)
    result = generate_roof(fp, 35.0)
    path = str(tmp_path / "vykaz.xlsx")
    # base 3.0 + wall 2.5 → eave 5.5 above ground
    write_design_report_xlsx(DesignResult([result]), path,
                             building_names=["Věž"], building_eaves=[5.5])
    rows = _rows(path)
    assert rows["Výška okapu nad terénem"] == 5.5
    assert rows["Výška hřebene nad terénem"] == round(5.5 + result.apex_height_m, 2)


def test_heights_rows_absent_without_eaves(tmp_path):
    fp = Footprint.from_lists(RECT, ["eave"] * 4)
    result = generate_roof(fp, 35.0)
    path = str(tmp_path / "vykaz.xlsx")
    write_design_report_xlsx(DesignResult([result]), path)
    rows = _rows(path)
    assert "Výška okapu nad terénem" not in rows


def test_editor_export_passes_heights(tmp_path):
    from PySide6.QtWidgets import QApplication
    from tectora.application.project import Project
    from tectora.gui.app_window import MainWindow
    from tectora.io.sqlite_library import SqliteProjectLibrary

    app = QApplication.instance() or QApplication([])
    lib = SqliteProjectLibrary(str(tmp_path / "lib.db"))
    w = MainWindow(library=lib)
    w.open_editor(Project(project_name="Test", global_pitch_deg=35.0))
    e = w._stack.widget(1)
    for (x, y) in RECT:
        e.canvas.add_point(x, y)
    e.canvas.close_polygon()
    e._eave_height_spin.setValue(5.5)

    path = str(tmp_path / "vykaz.xlsx")
    e._export_to(path)
    rows = _rows(path)
    assert rows["Výška okapu nad terénem"] == 5.5
