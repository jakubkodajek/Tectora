from tectora.application.project import Project
from tectora.core.footprint import Footprint


def sample():
    fp = Footprint.from_lists([(0, 0), (12, 0), (12, 8), (0, 8)],
                              ["eave", "gable", "eave", "gable"])
    return Project(project_name="Dům", footprint=fp, global_pitch_deg=35.0)


def test_to_dict_schema():
    d = sample().to_dict()
    assert d["schema_version"] == "1.0"
    assert d["units"] == "m"
    assert d["footprint"]["edges"][1] == {"type": "gable"}
    assert d["global_pitch_deg"] == 35.0


def test_roundtrip():
    p = Project.from_dict(sample().to_dict())
    assert p.project_name == "Dům"
    assert p.footprint.edges[1].type.value == "gable"
