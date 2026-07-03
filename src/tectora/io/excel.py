"""Excel I/O — input template and output report."""
from __future__ import annotations

from pathlib import Path

from openpyxl import Workbook, load_workbook
from openpyxl.styles import Alignment, Border, Font, PatternFill, Side
from openpyxl.utils import get_column_letter

from tectora.core.result import RoofResult
from tectora.core.spec import RoofSpec, RoofType
from tectora.io.json_io import spec_from_dict


# ---------- styles ----------
_HEADER_FILL = PatternFill("solid", fgColor="1F4E78")
_HEADER_FONT = Font(bold=True, color="FFFFFF", size=12)
_SUBHEADER_FILL = PatternFill("solid", fgColor="D9E1F2")
_SUBHEADER_FONT = Font(bold=True, color="1F4E78", size=11)
_LABEL_FONT = Font(bold=True)
_VALUE_FONT = Font(name="Consolas")
_BORDER = Border(
    left=Side(style="thin", color="BFBFBF"),
    right=Side(style="thin", color="BFBFBF"),
    top=Side(style="thin", color="BFBFBF"),
    bottom=Side(style="thin", color="BFBFBF"),
)


# ---------- input (input template) ----------
_INPUT_FIELDS = [
    ("Název projektu", "name", "", "text"),
    ("Typ střechy", "type", "gable", "select: shed | gable | hip | half_hip | pyramid"),
    ("Délka L (m)", "length", 10.0, "delší rozměr půdorysu"),
    ("Šířka W (m)", "width", 6.0, "kratší rozměr půdorysu"),
    ("Sklon (°)", "pitch", 30.0, "hlavní sklon, např. 25–45"),
    ("Sklon zadní (°)", "pitch_back", "", "volitelné, pro asymetrickou sedlovou"),
    ("Poměr valby (0–1)", "half_hip_ratio", 0.5, "jen pro polovalbovou"),
    ("Přesah okapu (m)", "eave_overhang", 0.0, ""),
]


def write_template_xlsx(path: Path) -> None:
    """Create a blank Excel input template for entering roof parameters."""
    wb = Workbook()
    ws = wb.active
    ws.title = "Vstup"

    ws["A1"] = "Tectora — vstupní šablona"
    ws["A1"].font = Font(bold=True, size=14, color="1F4E78")
    ws.merge_cells("A1:C1")

    ws["A3"] = "Parametr"
    ws["B3"] = "Hodnota"
    ws["C3"] = "Poznámka"
    for col in ("A3", "B3", "C3"):
        ws[col].fill = _HEADER_FILL
        ws[col].font = _HEADER_FONT
        ws[col].alignment = Alignment(horizontal="center")

    for i, (label, _key, default, hint) in enumerate(_INPUT_FIELDS, start=4):
        ws.cell(row=i, column=1, value=label).font = _LABEL_FONT
        cell = ws.cell(row=i, column=2, value=default)
        cell.font = _VALUE_FONT
        cell.fill = PatternFill("solid", fgColor="FFF2CC")
        cell.border = _BORDER
        ws.cell(row=i, column=3, value=hint).font = Font(italic=True, color="808080")

    ws.column_dimensions["A"].width = 24
    ws.column_dimensions["B"].width = 18
    ws.column_dimensions["C"].width = 48

    wb.save(path)


def read_spec_xlsx(path: Path) -> RoofSpec:
    """Read a spec from an Excel template."""
    wb = load_workbook(path, data_only=True)
    if "Vstup" in wb.sheetnames:
        ws = wb["Vstup"]
    else:
        ws = wb.active

    data: dict = {}
    for i, (_label, key, _default, _hint) in enumerate(_INPUT_FIELDS, start=4):
        val = ws.cell(row=i, column=2).value
        if val not in (None, ""):
            data[key] = val

    return spec_from_dict(data)


# ---------- output (report) ----------
def _result_rows(result: RoofResult) -> list[tuple]:
    """Per-building results rows ``(label, value, unit)``, rounded to 2 decimals.

    Shared by :func:`write_report_xlsx` (the single-roof "Výsledky" section) and
    :func:`write_design_report_xlsx` (each per-building section) so the two
    reports never drift.
    """
    return [
        ("Celková plocha střechy", round(result.total_area_m2, 2), "m²"),
        ("Délka hřebene", round(result.ridge_length_m, 2), "m"),
        ("Délka nároží (celkem)", round(result.hip_length_m, 2), "m"),
        ("Počet nároží", result.hip_count, "ks"),
        ("Délka úžlabí", round(result.valley_length_m, 2), "m"),
        ("Krajové tašky — levý štít", round(result.gable_edge_left_m, 2), "m"),
        ("Krajové tašky — pravý štít", round(result.gable_edge_right_m, 2), "m"),
        ("Krajové tašky — celkem", round(result.gable_edge_total_m, 2), "m"),
        ("Obvod okapové hrany", round(result.eaves_perimeter_m, 2), "m"),
        ("Výška hřebene nad okapem", round(result.apex_height_m, 2), "m"),
    ]


def write_report_xlsx(result: RoofResult, path: Path) -> None:
    """Nicely formatted report with input parameters and all results."""
    wb = Workbook()
    ws = wb.active
    ws.title = "Výkaz"

    row = 1
    # Header
    ws.cell(row=row, column=1, value="Tectora — výkaz výpočtu střechy").font = Font(
        bold=True, size=16, color="1F4E78"
    )
    ws.merge_cells(start_row=row, start_column=1, end_row=row, end_column=4)
    row += 2

    if result.spec.name:
        ws.cell(row=row, column=1, value="Projekt:").font = _LABEL_FONT
        ws.cell(row=row, column=2, value=result.spec.name)
        row += 1

    # Inputs
    row = _write_section(ws, row, "Vstupní parametry", [
        ("Typ střechy", f"{result.spec.type.cs_name} ({result.spec.type.value})", ""),
        ("Délka L", result.spec.length, "m"),
        ("Šířka W", result.spec.width, "m"),
        ("Sklon hlavní", result.spec.pitch, "°"),
        ("Sklon zadní",
         result.spec.pitch_back if result.spec.pitch_back is not None else "—", "°"),
        ("Poměr valby",
         result.spec.half_hip_ratio if result.spec.type == RoofType.HALF_HIP else "—", ""),
        ("Přesah okapu", result.spec.eave_overhang, "m"),
    ])

    # Results
    row = _write_section(ws, row + 1, "Výsledky", _result_rows(result))

    # Area per face
    row += 1
    row = _write_section(ws, row, "Plochy po rovinách", [
        (f.name, round(f.area_m2, 2), f"m² ({f.slope_deg:.1f}°)") for f in result.faces
    ])

    if result.notes:
        row += 1
        ws.cell(row=row, column=1, value="Poznámky").font = _SUBHEADER_FONT
        ws.cell(row=row, column=1).fill = _SUBHEADER_FILL
        ws.merge_cells(start_row=row, start_column=1, end_row=row, end_column=4)
        row += 1
        for note in result.notes:
            ws.cell(row=row, column=1, value=note).font = Font(italic=True)
            ws.merge_cells(start_row=row, start_column=1, end_row=row, end_column=4)
            row += 1

    ws.column_dimensions["A"].width = 34
    ws.column_dimensions["B"].width = 16
    ws.column_dimensions["C"].width = 12
    ws.column_dimensions["D"].width = 4

    wb.save(path)


def write_design_report_xlsx(design, path: Path, building_names=None,
                             building_eaves=None) -> None:
    """Výkaz výměr for a whole multi-building design.

    One "Výkaz" worksheet with a per-building "Výsledky" section for each
    building (reusing :func:`_result_rows`) followed by a "Celkem za návrh"
    section with the design totals. ``design`` is a
    :class:`tectora.application.use_cases.DesignResult`; ``building_names`` is an
    optional list aligned with ``design.buildings`` — entries that are falsy
    (``None``/``""``) fall back to ``Budova N``. ``building_eaves`` is an
    optional aligned list of absolute eave heights above ground (base
    elevation + wall height); when given, each section also reports the eave
    and ridge heights above ground.
    """
    wb = Workbook()
    ws = wb.active
    ws.title = "Výkaz"

    row = 1
    ws.cell(row=row, column=1, value="Tectora — výkaz výměr (návrh)").font = Font(
        bold=True, size=16, color="1F4E78"
    )
    ws.merge_cells(start_row=row, start_column=1, end_row=row, end_column=4)
    row += 2

    names = list(building_names) if building_names is not None else []
    eaves = list(building_eaves) if building_eaves is not None else []
    for i, result in enumerate(design.buildings):
        name = names[i] if i < len(names) else None
        title = (name or "").strip() if isinstance(name, str) else name
        if not title:
            title = f"Budova {i + 1}"
        rows = _result_rows(result)
        if i < len(eaves) and eaves[i] is not None:
            eave = float(eaves[i])
            rows = rows + [
                ("Výška okapu nad terénem", round(eave, 2), "m"),
                ("Výška hřebene nad terénem",
                 round(eave + result.apex_height_m, 2), "m"),
            ]
        row = _write_section(ws, row, title, rows)
        row += 1

    # Design totals
    _write_section(ws, row, "Celkem za návrh", [
        ("Celková plocha střechy", round(design.total_area_m2, 2), "m²"),
        ("Obvod okapové hrany", round(design.total_eaves_perimeter_m, 2), "m"),
        ("Délka hřebene", round(design.total_ridge_length_m, 2), "m"),
        ("Délka nároží (celkem)", round(design.total_hip_length_m, 2), "m"),
        ("Délka úžlabí", round(design.total_valley_length_m, 2), "m"),
        ("Krajové tašky — celkem", round(design.total_gable_edge_m, 2), "m"),
    ])

    ws.column_dimensions["A"].width = 34
    ws.column_dimensions["B"].width = 16
    ws.column_dimensions["C"].width = 12
    ws.column_dimensions["D"].width = 4

    wb.save(path)


def _write_section(ws, row: int, title: str, rows: list[tuple]) -> int:
    cell = ws.cell(row=row, column=1, value=title)
    cell.font = _SUBHEADER_FONT
    cell.fill = _SUBHEADER_FILL
    ws.merge_cells(start_row=row, start_column=1, end_row=row, end_column=4)
    row += 1
    for label, value, unit in rows:
        ws.cell(row=row, column=1, value=label).font = _LABEL_FONT
        v_cell = ws.cell(row=row, column=2, value=value)
        v_cell.font = _VALUE_FONT
        v_cell.alignment = Alignment(horizontal="right")
        v_cell.border = _BORDER
        ws.cell(row=row, column=3, value=unit).font = Font(color="808080")
        row += 1
    return row
