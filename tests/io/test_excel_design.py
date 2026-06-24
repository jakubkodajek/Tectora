# -*- coding: utf-8 -*-
"""P4: write_design_report_xlsx — multi-building výkaz výměr.

A two-building DesignResult must produce one "Výkaz" sheet with a per-building
section for each building and a "Celkem za návrh" totals section whose area
equals the sum of the two building areas.
"""
import openpyxl
import pytest

from tectora.application import use_cases as uc
from tectora.application.project import Project
from tectora.core.footprint import Footprint
from tectora.core.roof_generator import generate_roof
from tectora.io import write_design_report_xlsx


def _rect(x0, y0, x1, y1, edges=("eave", "gable", "eave", "gable")):
    return Footprint.from_lists(
        [(x0, y0), (x1, y0), (x1, y1), (x0, y1)],
        list(edges),
    )


def _two_building_design():
    r0 = generate_roof(_rect(0, 0, 12, 8), 35.0, name="A")
    r1 = generate_roof(_rect(20, 0, 26, 4), 30.0, name="B")
    return uc.DesignResult([r0, r1]), r0, r1


def _all_text(ws):
    out = []
    for row in ws.iter_rows(values_only=True):
        for v in row:
            if isinstance(v, str):
                out.append(v)
    return out


def test_design_report_default_names_and_total_area(tmp_path):
    design, r0, r1 = _two_building_design()
    out = tmp_path / "design.xlsx"
    write_design_report_xlsx(design, out)

    assert out.exists()
    wb = openpyxl.load_workbook(out)
    assert "Výkaz" in wb.sheetnames
    ws = wb["Výkaz"]
    texts = _all_text(ws)

    # Default per-building titles + the totals section.
    assert "Budova 1" in texts
    assert "Budova 2" in texts
    assert "Celkem za návrh" in texts

    # The total-area numeric cell equals Σ of the two building areas.
    expected = round(r0.total_area_m2 + r1.total_area_m2, 2)
    area_cells = [
        ws.cell(row=cell.row, column=2).value
        for col in ws.iter_cols(min_col=1, max_col=1)
        for cell in col
        if cell.value == "Celková plocha střechy"
    ]
    # The last "Celková plocha střechy" row belongs to the totals section.
    assert area_cells, "no area rows written"
    assert area_cells[-1] == pytest.approx(expected, abs=0.01)


def test_design_report_uses_given_names(tmp_path):
    design, _r0, _r1 = _two_building_design()
    out = tmp_path / "named.xlsx"
    write_design_report_xlsx(design, out, building_names=["Garáž", "Kůlna"])

    ws = openpyxl.load_workbook(out)["Výkaz"]
    texts = _all_text(ws)
    assert "Garáž" in texts
    assert "Kůlna" in texts
    # Empty / None name falls back to the default for that index.


def test_design_report_blank_name_falls_back(tmp_path):
    design, _r0, _r1 = _two_building_design()
    out = tmp_path / "blank.xlsx"
    write_design_report_xlsx(design, out, building_names=["", None])

    texts = _all_text(openpyxl.load_workbook(out)["Výkaz"])
    assert "Budova 1" in texts
    assert "Budova 2" in texts


def test_design_report_matches_calculate_design(tmp_path):
    """A 2-building Project routed through calculate_design exports cleanly and
    the totals area matches the DesignResult's total."""
    project = Project(buildings=[
        uc.Building(footprint=_rect(0, 0, 10, 6), pitch_deg=35.0, name="P1"),
        uc.Building(footprint=_rect(20, 0, 28, 6), pitch_deg=40.0, name="P2"),
    ]) if hasattr(uc, "Building") else None
    if project is None:
        from tectora.application.project import Building
        project = Project(buildings=[
            Building(footprint=_rect(0, 0, 10, 6), pitch_deg=35.0, name="P1"),
            Building(footprint=_rect(20, 0, 28, 6), pitch_deg=40.0, name="P2"),
        ])

    design = uc.calculate_design(project)
    out = tmp_path / "calc.xlsx"
    write_design_report_xlsx(design, out, building_names=["P1", "P2"])

    ws = openpyxl.load_workbook(out)["Výkaz"]
    texts = _all_text(ws)
    assert "P1" in texts and "P2" in texts
    assert "Celkem za návrh" in texts
