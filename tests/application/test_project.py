from tectora.application.project import Building, Project
from tectora.core.footprint import Footprint


def sample():
    fp = Footprint.from_lists([(0, 0), (12, 0), (12, 8), (0, 8)],
                              ["eave", "gable", "eave", "gable"])
    return Project(project_name="Dům", footprint=fp, global_pitch_deg=35.0)


def test_to_dict_schema_v2():
    d = sample().to_dict()
    assert d["schema_version"] == "2.0"
    assert d["units"] == "m"
    # Single-building project serialises as a one-element buildings array.
    assert len(d["buildings"]) == 1
    b = d["buildings"][0]
    assert b["footprint"]["edges"][1] == {"type": "gable"}
    assert b["pitch_deg"] == 35.0


def test_roundtrip():
    p = Project.from_dict(sample().to_dict())
    assert p.project_name == "Dům"
    assert p.footprint.edges[1].type.value == "gable"


def test_legacy_properties_delegate_to_first_building():
    p = sample()
    assert p.global_pitch_deg == 35.0
    assert p.footprint is not None
    p.global_pitch_deg = 40.0
    p.overhang_m = 0.5
    assert p.buildings[0].pitch_deg == 40.0
    assert p.buildings[0].overhang_m == 0.5


def test_load_legacy_schema_1_0():
    # A pre-existing 1.0 dict (single top-level footprint) loads as one building.
    legacy = {
        "schema_version": "1.0",
        "project_name": "Starý",
        "units": "m",
        "footprint": {
            "vertices": [[0, 0], [10, 0], [10, 6], [0, 6]],
            "edges": [{"type": "eave"}] * 4,
        },
        "global_pitch_deg": 30.0,
        "overhang_m": 0.4,
        "wall_height_m": 2.7,
    }
    p = Project.from_dict(legacy)
    assert len(p.buildings) == 1
    assert p.footprint is not None and p.footprint.n == 4
    assert p.global_pitch_deg == 30.0
    assert p.overhang_m == 0.4
    assert p.wall_height_m == 2.7


def test_multi_building_roundtrip():
    fp1 = Footprint.from_lists([(0, 0), (8, 0), (8, 5), (0, 5)], ["eave"] * 4)
    fp2 = Footprint.from_lists([(20, 0), (26, 0), (26, 4), (20, 4)],
                               ["gable", "eave", "gable", "eave"])
    p = Project(
        project_name="Usedlost",
        buildings=[
            Building(footprint=fp1, pitch_deg=35.0, name="Dům"),
            Building(footprint=fp2, pitch_deg=22.0, overhang_m=0.6, name="Garáž"),
        ],
    )
    d = p.to_dict()
    assert len(d["buildings"]) == 2
    p2 = Project.from_dict(d)
    assert len(p2.buildings) == 2
    assert p2.buildings[1].name == "Garáž"
    assert p2.buildings[1].pitch_deg == 22.0
    assert p2.buildings[1].overhang_m == 0.6
    assert p2.buildings[1].footprint.edges[0].type.value == "gable"
    # Legacy property points at the first building.
    assert p2.global_pitch_deg == 35.0


def test_empty_project_has_one_empty_building():
    p = Project()
    assert len(p.buildings) == 1
    assert p.footprint is None
    # Round-trips without error.
    assert Project.from_dict(p.to_dict()).footprint is None
