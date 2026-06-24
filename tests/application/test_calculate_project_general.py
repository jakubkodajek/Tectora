import pytest
from tectora.application import use_cases as uc
from tectora.application.project import Project
from tectora.core.footprint import Footprint


def test_calculate_project_handles_lshape():
    fp = Footprint.from_lists(
        [(0, 0), (6, 0), (6, 3), (3, 3), (3, 5), (0, 5)], ["eave"] * 6)
    r = uc.calculate_project(Project(project_name="L", footprint=fp, global_pitch_deg=35.0))
    assert r.valley_length_m > 0
