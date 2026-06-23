import pytest
from tectora.application import use_cases as uc
from tectora.application.project import Project
from tectora.core.footprint import Footprint
from tectora.core.result import RoofResult


def gable_project():
    fp = Footprint.from_lists([(0, 0), (12, 0), (12, 8), (0, 8)],
                              ["eave", "gable", "eave", "gable"])
    return Project(project_name="Dům", footprint=fp, global_pitch_deg=35.0)


def test_calculate_project_reuses_core():
    r = uc.calculate_project(gable_project())
    assert isinstance(r, RoofResult)
    assert r.ridge_length_m == pytest.approx(12.0)


class _FakeExporter:
    def __init__(self):
        self.calls = []

    def export(self, result, path):
        self.calls.append((result, path))


def test_export_report_uses_port():
    r = uc.calculate_project(gable_project())
    exp = _FakeExporter()
    uc.export_report(r, exp, "out.xlsx")
    assert exp.calls == [(r, "out.xlsx")]
