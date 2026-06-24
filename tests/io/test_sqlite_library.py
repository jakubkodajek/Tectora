import pytest
from tectora.application.library import EntryNotFound, LibraryEntry, now_iso
from tectora.application.project import Project
from tectora.core.footprint import Footprint
from tectora.core.location import Location
from tectora.io.sqlite_library import SqliteProjectLibrary


def make_entry(name="Dům"):
    fp = Footprint.from_lists([(0, 0), (12, 0), (12, 8), (0, 8)],
                              ["eave", "gable", "eave", "gable"])
    ts = now_iso()
    return LibraryEntry(None, name, "poznámka",
                        Location("Brno", 49.19, 16.61),
                        Project(project_name=name, footprint=fp, global_pitch_deg=35.0),
                        {"results": {"total_area_m2": 117.0}}, ts, ts)


def lib(tmp_path):
    return SqliteProjectLibrary(str(tmp_path / "lib.db"))


def test_add_get_roundtrip(tmp_path):
    L = lib(tmp_path)
    eid = L.add(make_entry())
    got = L.get(eid)
    assert got.name == "Dům"
    assert got.note == "poznámka"
    assert got.location.name == "Brno"
    assert got.project.footprint.edges[1].type.value == "gable"
    assert got.result["results"]["total_area_m2"] == 117.0


def test_list_returns_all(tmp_path):
    L = lib(tmp_path)
    L.add(make_entry("A"))
    L.add(make_entry("B"))
    names = {e.name for e in L.list()}
    assert names == {"A", "B"}


def test_update(tmp_path):
    L = lib(tmp_path)
    eid = L.add(make_entry("Old"))
    entry = L.get(eid)
    entry.name = "New"
    entry.location = None
    L.update(eid, entry)
    assert L.get(eid).name == "New"
    assert L.get(eid).location is None


def test_delete(tmp_path):
    L = lib(tmp_path)
    eid = L.add(make_entry())
    L.delete(eid)
    with pytest.raises(EntryNotFound):
        L.get(eid)


def test_get_missing_raises(tmp_path):
    with pytest.raises(EntryNotFound):
        lib(tmp_path).get(999)


def test_multi_building_entry_roundtrip(tmp_path):
    from tectora.application.project import Building
    fp1 = Footprint.from_lists([(0, 0), (8, 0), (8, 5), (0, 5)], ["eave"] * 4)
    fp2 = Footprint.from_lists([(20, 0), (26, 0), (26, 4), (20, 4)], ["eave"] * 4)
    proj = Project(project_name="Statek", buildings=[
        Building(footprint=fp1, pitch_deg=35.0, name="Dům"),
        Building(footprint=fp2, pitch_deg=18.0, overhang_m=0.6, name="Stodola"),
    ])
    ts = now_iso()
    L = lib(tmp_path)
    eid = L.add(LibraryEntry(None, "Statek", "", None, proj, None, ts, ts))
    got = L.get(eid)
    assert len(got.project.buildings) == 2
    assert [b.name for b in got.project.buildings] == ["Dům", "Stodola"]
    assert got.project.buildings[1].pitch_deg == 18.0
    assert got.project.buildings[1].overhang_m == 0.6
