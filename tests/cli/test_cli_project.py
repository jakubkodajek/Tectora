"""CLI: schema --project, calculate --project, validate."""
import json

from tectora.application.project import Building, Project
from tectora.cli.main import main
from tectora.core.footprint import Footprint

RECT_A = [(0, 0), (10, 0), (10, 6), (0, 6)]
RECT_B = [(20, 0), (26, 0), (26, 4), (20, 4)]
L_SHAPE = [(0, 0), (6, 0), (6, 3), (3, 3), (3, 6), (0, 6)]


def _write_project(tmp_path, buildings, name="Test"):
    project = Project(project_name=name, buildings=buildings)
    path = tmp_path / "projekt.tectora"
    path.write_text(json.dumps(project.to_dict(), ensure_ascii=False),
                    encoding="utf-8")
    return path


def _rect_building(pts=RECT_A, **kw):
    return Building(footprint=Footprint.from_lists(pts, ["eave"] * len(pts)), **kw)


# ---------------------------------------------------------------------------
# schema --project
# ---------------------------------------------------------------------------

def test_schema_project_describes_buildings(capsys):
    assert main(["schema", "--project"]) == 0
    schema = json.loads(capsys.readouterr().out)
    assert "buildings" in schema["properties"]
    b = schema["properties"]["buildings"]["items"]["properties"]
    assert "eave_height_m" in b and "half_hip_m" in b and "footprint" in b


def test_schema_without_flag_stays_parametric(capsys):
    assert main(["schema"]) == 0
    schema = json.loads(capsys.readouterr().out)
    assert "length" in schema["properties"]      # the old RoofSpec schema


# ---------------------------------------------------------------------------
# calculate --project
# ---------------------------------------------------------------------------

def test_calculate_project_outputs_design_json(tmp_path, capsys):
    path = _write_project(tmp_path, [
        _rect_building(RECT_A, pitch_deg=35.0, name="Dům"),
        _rect_building(RECT_B, pitch_deg=30.0, eave_height_m=5.5),
    ])
    assert main(["calculate", "--project", str(path)]) == 0
    out = json.loads(capsys.readouterr().out)
    assert len(out["buildings"]) == 2
    assert out["buildings"][0]["name"] == "Dům"
    assert out["buildings"][1]["eave_height_m"] == 5.5
    per = sum(b["results"]["total_area_m2"] for b in out["buildings"])
    assert abs(out["totals"]["total_area_m2"] - per) < 0.01


def test_calculate_project_writes_design_xlsx(tmp_path):
    from openpyxl import load_workbook
    path = _write_project(tmp_path, [_rect_building()])
    out = tmp_path / "vykaz.xlsx"
    assert main(["calculate", "--project", str(path), "-o", str(out), "-q"]) == 0
    labels = [row[0].value
              for row in load_workbook(out)["Výkaz"].iter_rows(min_col=1, max_col=1)]
    assert "Celkem za návrh" in labels
    assert "Materiál (odhad)" in labels
    assert "Výška okapu nad terénem" in labels


def test_calculate_project_fails_loudly_on_bad_building(tmp_path, capsys):
    fp = Footprint.from_lists(L_SHAPE, ["eave"] * 6)
    data = Project(project_name="X", buildings=[Building(footprint=fp)]).to_dict()
    # Corrupt: mark a long mid-edge as a gable — not a valid wing-end cap.
    data["buildings"][0]["footprint"]["edges"][0] = {"type": "gable"}
    path = tmp_path / "bad.tectora"
    path.write_text(json.dumps(data), encoding="utf-8")
    assert main(["calculate", "--project", str(path)]) == 2
    assert "Chyba" in capsys.readouterr().err


def test_calculate_project_and_spec_are_exclusive(tmp_path, capsys):
    path = _write_project(tmp_path, [_rect_building()])
    assert main(["calculate", "--project", str(path), "--spec", str(path)]) == 2


# ---------------------------------------------------------------------------
# validate
# ---------------------------------------------------------------------------

def test_validate_ok(tmp_path, capsys):
    path = _write_project(tmp_path, [_rect_building()])
    assert main(["validate", str(path)]) == 0
    assert "OK" in capsys.readouterr().out


def test_validate_reports_invalid_gable(tmp_path, capsys):
    fp = Footprint.from_lists(L_SHAPE, ["eave"] * 6)
    data = Project(project_name="X", buildings=[Building(footprint=fp)]).to_dict()
    data["buildings"][0]["footprint"]["edges"][0] = {"type": "gable"}
    path = tmp_path / "bad.tectora"
    path.write_text(json.dumps(data), encoding="utf-8")
    assert main(["validate", str(path)]) == 2
    assert "štít" in capsys.readouterr().out


def test_validate_json_output(tmp_path, capsys):
    data = {"buildings": [{"footprint": {
        "vertices": [[0, 0], [4, 0], [0, 4], [4, 4]],   # bowtie — self-intersects
        "edges": [{"type": "eave"}] * 4,
    }}]}
    path = tmp_path / "bowtie.json"
    path.write_text(json.dumps(data), encoding="utf-8")
    assert main(["validate", str(path), "--json"]) == 2
    out = json.loads(capsys.readouterr().out)
    assert out["valid"] is False
    assert out["problems"]


def test_validate_overlap_is_warning_not_error(tmp_path, capsys):
    path = _write_project(tmp_path, [
        _rect_building(RECT_A),
        _rect_building([(5, 3), (12, 3), (12, 9), (5, 9)]),
    ])
    assert main(["validate", str(path)]) == 0
    out = capsys.readouterr().out
    assert "překrývají" in out and "OK" in out


def test_validate_missing_file(tmp_path):
    assert main(["validate", str(tmp_path / "neni.json")]) == 2


def test_validate_bad_pitch(tmp_path, capsys):
    path = _write_project(tmp_path, [_rect_building(pitch_deg=95.0)])
    assert main(["validate", str(path)]) == 2
    assert "sklon" in capsys.readouterr().out
