from tectora.application.library import LibraryEntry, entry_summary, now_iso
from tectora.application.project import Project
from tectora.core.location import Location


def make_entry():
    return LibraryEntry(
        id=7, name="Dům Novák", note="valba",
        location=Location("Brno", 49.19, 16.61),
        project=Project(project_name="Dům Novák"),
        result={"results": {"total_area_m2": 117.0}},
        created_at=now_iso(), updated_at=now_iso(),
    )


def test_now_iso_is_string():
    assert isinstance(now_iso(), str) and "T" in now_iso()


def test_entry_summary_extracts_area_and_location():
    s = entry_summary(make_entry())
    assert s["id"] == 7
    assert s["name"] == "Dům Novák"
    assert s["location"] == "Brno"
    assert s["area_m2"] == 117.0


def test_entry_summary_handles_missing_result_and_location():
    e = make_entry()
    e.result = None
    e.location = None
    s = entry_summary(e)
    assert s["area_m2"] is None
    assert s["location"] is None
