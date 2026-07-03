"""Design report: optional 'Materiál (odhad)' section."""
import os
os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

from openpyxl import load_workbook

from tectora.application.use_cases import DesignResult
from tectora.core.footprint import Footprint
from tectora.core.materials import MaterialSpec
from tectora.core.roof_generator import generate_roof
from tectora.io.excel import write_design_report_xlsx

RECT = [(0, 0), (10, 0), (10, 6), (0, 6)]


def _col_a(path):
    ws = load_workbook(path)["Výkaz"]
    return [row[0].value for row in ws.iter_rows(min_col=1, max_col=1)
            if row[0].value is not None]


def test_material_section_present_when_spec_given(tmp_path):
    fp = Footprint.from_lists(RECT, ["eave"] * 4)
    design = DesignResult([generate_roof(fp, 35.0)])
    path = str(tmp_path / "vykaz.xlsx")
    write_design_report_xlsx(design, path, material_spec=MaterialSpec())
    labels = _col_a(path)
    assert "Materiál (odhad)" in labels
    assert any(l.startswith("Tašky") for l in labels)
    assert any(l.startswith("Předpoklady:") for l in labels)


def test_material_section_absent_by_default(tmp_path):
    fp = Footprint.from_lists(RECT, ["eave"] * 4)
    design = DesignResult([generate_roof(fp, 35.0)])
    path = str(tmp_path / "vykaz.xlsx")
    write_design_report_xlsx(design, path)
    assert "Materiál (odhad)" not in _col_a(path)


def test_editor_export_includes_material(tmp_path):
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

    path = str(tmp_path / "vykaz.xlsx")
    e._export_to(path)
    assert "Materiál (odhad)" in _col_a(path)
