"""End-to-end tests: full pipelines from input to output.

Two dimensions:
1. Real CLI via subprocess (executable `tectora` command, parametric input).
2. New hexagonal flow: drawn polygon (Footprint) → calculate_project →
   RoofResult → serialisation / export via port.
"""
import json
import subprocess
import sys

import openpyxl
import pytest

CLI = [sys.executable, "-m", "tectora.cli.main"]


def run_cli(args):
    return subprocess.run(
        CLI + args, capture_output=True, text=True, encoding="utf-8"
    )


class TestCliEndToEnd:
    def test_calculate_inline_prints_json(self):
        r = run_cli(["calculate", "--type", "gable",
                     "--length", "12", "--width", "8", "--pitch", "35"])
        assert r.returncode == 0, r.stderr
        data = json.loads(r.stdout)
        assert data["input"]["type"] == "gable"
        assert data["results"]["ridge_length_m"] == pytest.approx(12.0)
        assert data["results"]["total_area_m2"] > 0

    def test_calculate_from_json_spec_to_json_file(self, tmp_path):
        spec = tmp_path / "roof.json"
        spec.write_text(
            json.dumps({"type": "hip", "length": 10, "width": 6, "pitch": 30}),
            encoding="utf-8",
        )
        out = tmp_path / "result.json"
        r = run_cli(["calculate", "--spec", str(spec), "-o", str(out), "--quiet"])
        assert r.returncode == 0, r.stderr
        result = json.loads(out.read_text(encoding="utf-8"))
        assert result["results"]["hip_count"] == 4
        assert result["results"]["total_area_m2"] > 0

    def test_calculate_to_xlsx_report(self, tmp_path):
        out = tmp_path / "report.xlsx"
        r = run_cli(["calculate", "--type", "pyramid",
                     "--length", "8", "--width", "8", "--pitch", "30",
                     "-o", str(out), "--quiet"])
        assert r.returncode == 0, r.stderr
        assert out.exists()
        assert "Výkaz" in openpyxl.load_workbook(out).sheetnames

    def test_template_creates_xlsx(self, tmp_path):
        out = tmp_path / "template.xlsx"
        r = run_cli(["template", "-o", str(out)])
        assert r.returncode == 0, r.stderr
        assert out.exists()
        assert "Vstup" in openpyxl.load_workbook(out).sheetnames

    def test_schema_is_valid_json(self):
        r = run_cli(["schema"])
        assert r.returncode == 0, r.stderr
        schema = json.loads(r.stdout)
        assert "gable" in schema["properties"]["type"]["enum"]

    def test_types_lists_all_roof_types(self):
        r = run_cli(["types"])
        assert r.returncode == 0, r.stderr
        for t in ("shed", "gable", "hip", "half_hip", "pyramid"):
            assert t in r.stdout

    def test_invalid_pitch_returns_nonzero(self):
        r = run_cli(["calculate", "--type", "gable",
                     "--length", "10", "--width", "6", "--pitch", "0"])
        assert r.returncode != 0


class TestHexagonalPipelineEndToEnd:
    """Flow through the new layer: Footprint → use-case → result/export."""

    def _gable_project(self):
        from tectora.application.project import Project
        from tectora.core.footprint import Footprint

        fp = Footprint.from_lists(
            [(0, 0), (12, 0), (12, 8), (0, 8)],
            ["eave", "gable", "eave", "gable"],
        )
        return Project(project_name="Dům Novák", footprint=fp, global_pitch_deg=35.0)

    def test_footprint_to_result(self):
        from tectora.application import use_cases as uc

        result = uc.calculate_project(self._gable_project())
        assert result.ridge_length_m == pytest.approx(12.0)
        as_dict = result.as_dict()
        assert as_dict["results"]["total_area_m2"] > 0

    def test_project_json_roundtrip_gives_same_result(self):
        from tectora.application import use_cases as uc
        from tectora.application.project import Project

        original = self._gable_project()
        reloaded = Project.from_dict(original.to_dict())
        assert uc.calculate_project(reloaded).total_area_m2 == pytest.approx(
            uc.calculate_project(original).total_area_m2
        )

    def test_export_via_report_port_creates_xlsx(self, tmp_path):
        from tectora.application import use_cases as uc
        from tectora.io import write_report_xlsx

        class ExcelExporter:
            def export(self, result, path):
                write_report_xlsx(result, path)

        result = uc.calculate_project(self._gable_project())
        out = tmp_path / "vykaz.xlsx"
        uc.export_report(result, ExcelExporter(), out)
        assert out.exists()
        assert "Výkaz" in openpyxl.load_workbook(out).sheetnames

    def test_unsupported_polygon_is_rejected(self):
        from tectora.application import use_cases as uc
        from tectora.application.project import Project
        from tectora.core.bridge import UnsupportedFootprint
        from tectora.core.footprint import Footprint

        l_shape = Footprint.from_lists(
            [(0, 0), (10, 0), (10, 6), (5, 6), (5, 10), (0, 10)],
            ["eave"] * 6,
        )
        project = Project(project_name="L", footprint=l_shape, global_pitch_deg=35.0)
        with pytest.raises(UnsupportedFootprint):
            uc.calculate_project(project)


class TestLibraryCliEndToEnd:
    """Real `tectora library` command over a temporary SQLite DB."""

    def _write_project(self, tmp_path, name="Dům"):
        from tectora.application.project import Project
        from tectora.core.footprint import Footprint

        fp = Footprint.from_lists(
            [(0, 0), (12, 0), (12, 8), (0, 8)],
            ["eave", "gable", "eave", "gable"],
        )
        p = tmp_path / "proj.json"
        p.write_text(
            json.dumps(Project(project_name=name, footprint=fp,
                               global_pitch_deg=35.0).to_dict()),
            encoding="utf-8",
        )
        return str(p)

    def test_library_add_then_list_via_cli(self, tmp_path):
        db = str(tmp_path / "lib.db")
        proj = self._write_project(tmp_path)

        r = run_cli(["library", "add", proj, "--db", db,
                     "--name", "Dům Novák", "--location", "Brno"])
        assert r.returncode == 0, r.stderr
        assert json.loads(r.stdout)["id"] == 1

        r = run_cli(["library", "list", "--db", db])
        assert r.returncode == 0, r.stderr
        rows = json.loads(r.stdout)
        assert len(rows) == 1
        assert rows[0]["name"] == "Dům Novák"
        assert rows[0]["location"] == "Brno"
        assert rows[0]["area_m2"] is not None

    def test_geocode_offline_returns_empty(self, monkeypatch, capsys):
        from tectora.cli.main import main
        from tectora.io import nominatim_geocoder as ng

        monkeypatch.setattr(ng.NominatimGeocoder, "search",
                            lambda self, q, limit=5: [])
        assert main(["geocode", "Mor"]) == 0
        assert json.loads(capsys.readouterr().out) == []
