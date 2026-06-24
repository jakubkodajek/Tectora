import pytest
from tectora.application import use_cases as uc
from tectora.application.library import EntryNotFound, LibraryEntry
from tectora.application.project import Project
from tectora.core.location import Location


class FakeLibrary:
    def __init__(self):
        self._rows = {}
        self._next = 1

    def add(self, entry):
        entry.id = self._next
        self._rows[self._next] = entry
        self._next += 1
        return entry.id

    def get(self, entry_id):
        if entry_id not in self._rows:
            raise EntryNotFound(str(entry_id))
        return self._rows[entry_id]

    def list(self):
        return list(self._rows.values())

    def update(self, entry_id, entry):
        if entry_id not in self._rows:
            raise EntryNotFound(str(entry_id))
        self._rows[entry_id] = entry

    def delete(self, entry_id):
        self._rows.pop(entry_id, None)


class FakeGeocoder:
    def search(self, query, limit=5):
        return [Location("Moravský Krumlov", 49.05, 16.31),
                Location("Moravské Budějovice", 49.05, 15.81)][:limit]


def test_save_and_list_and_load():
    lib = FakeLibrary()
    eid = uc.save_entry(lib, Project(project_name="Dům"), "Dům", note="x")
    assert eid == 1
    assert len(uc.list_entries(lib)) == 1
    loaded = uc.load_entry(lib, eid)
    assert loaded.name == "Dům"
    assert loaded.created_at == loaded.updated_at


def test_update_changes_fields_and_bumps_timestamp():
    lib = FakeLibrary()
    eid = uc.save_entry(lib, Project(), "Old")
    before = uc.load_entry(lib, eid).updated_at
    updated = uc.update_entry(lib, eid, name="New",
                              location=Location("Brno", 49.19, 16.61))
    assert updated.name == "New"
    assert updated.location.name == "Brno"
    assert updated.updated_at >= before


def test_delete_removes_entry():
    lib = FakeLibrary()
    eid = uc.save_entry(lib, Project(), "X")
    uc.delete_entry(lib, eid)
    with pytest.raises(EntryNotFound):
        uc.load_entry(lib, eid)


def test_suggest_location():
    results = uc.suggest_location(FakeGeocoder(), "Mor", limit=1)
    assert len(results) == 1
    assert results[0].name == "Moravský Krumlov"
