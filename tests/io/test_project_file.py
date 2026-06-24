from tectora.application.project import Project
from tectora.core.footprint import Footprint
from tectora.core.location import Location
from tectora.io.project_file import load_project_file, save_project_file


def test_roundtrip(tmp_path):
    fp = Footprint.from_lists([(0, 0), (12, 0), (12, 8), (0, 8)],
                              ["eave", "gable", "eave", "gable"])
    p = Project(project_name="Dům", footprint=fp, global_pitch_deg=35.0,
                overhang_m=0.5, wall_height_m=3.0)
    path = str(tmp_path / "x.tectora")
    save_project_file(path, p, note="pozn", location=Location("Brno", 49.2, 16.6))
    p2, note, loc = load_project_file(path)
    assert p2.project_name == "Dům"
    assert p2.global_pitch_deg == 35.0
    assert p2.overhang_m == 0.5
    assert p2.wall_height_m == 3.0
    assert p2.footprint.edges[1].type.value == "gable"
    assert note == "pozn"
    assert loc.name == "Brno"


def test_cli_compatible_keys(tmp_path):
    # The saved file's project keys are a superset usable by Project.from_dict
    p = Project(project_name="X", global_pitch_deg=30.0)
    path = str(tmp_path / "y.tectora")
    save_project_file(path, p)
    import json
    data = json.loads(open(path, encoding="utf-8").read())
    assert data["schema_version"] == "1.0"
    assert "footprint" in data and "global_pitch_deg" in data
