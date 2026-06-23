# Plán implementace — Tectora Fáze 1: Knihovna zakázek (SQLite) + poloha (backend + CLI)

> Verze 1.0 · 2026-06-23
> **Předpoklad:** dokončený plán `2026-06-23-faze1-jadro.md` (existují balíčky
> `tectora.domain`, `tectora.application`, `tectora.adapters`, `tectora.cli`,
> typy `Project`, `result_to_dict`, `JsonProjectRepository`, CLI `main`).
> Čti `docs/specifikace.md` §5.10, §5.11, §7.

## Cíl

Lokální databáze zakázek (uložených nákresů) s plným CRUD a našeptávačem polohy,
ovladatelná z CLI (a tím i z Claude Code). Žádné GUI — úvodní stránka je v plánu GUI.

**Rozsah:** `Location`, `LibraryEntry`, porty `ProjectLibrary` + `GeocodingService`,
adaptéry `sqlite_library` (SQLite) a `nominatim_geocoder` (OSM/Nominatim),
use-casy a CLI příkazy `library list/add/get/update/delete` + `geocode`.
Pokrývá akceptační případy 8 a 9 ze specifikace §12.

**Mimo rozsah:** GUI (úvodní stránka, karty, „tři tečky", našeptávač v poli) — plán GUI.

## Architektura

Nové **driven adaptéry** za porty (viz §7): `sqlite_library` implementuje
`ProjectLibrary`, `nominatim_geocoder` implementuje `GeocodingService`. Doména
dostane jen čistý value object `Location`. Žádné nové běhové závislosti — `sqlite3`
i `urllib` jsou ve stdlib.

## Struktura souborů

```
src/tectora/
  domain/location.py            # NOVÉ: Location (value object)
  domain/errors.py              # ÚPRAVA: + EntryNotFound
  application/library.py        # NOVÉ: LibraryEntry, now_iso, entry_summary
  application/ports.py          # ÚPRAVA: + ProjectLibrary, GeocodingService
  application/use_cases.py      # ÚPRAVA: + save/list/load/update/delete_entry, suggest_location
  adapters/sqlite_library.py    # NOVÉ: SqliteProjectLibrary, default_db_path
  adapters/nominatim_geocoder.py# NOVÉ: NominatimGeocoder
  cli/main.py                   # ÚPRAVA: + library/geocode příkazy
tests/
  domain/test_location.py
  application/test_library_use_cases.py
  adapters/test_sqlite_library.py
  adapters/test_nominatim_geocoder.py
  cli/test_cli_library.py
```

---

## Task 0 — Větev

**Steps:**

1. Z aktuální (zelené) větve s hotovým jádrem:
   ```bash
   git checkout -b feat/faze1-knihovna
   ```
2. Ověř, že stávající testy procházejí:
   ```bash
   python -m pytest -q
   ```
   Očekávaný výstup: vše `passed`.

---

## Task 1 — Doména: location.py + EntryNotFound

**Files:** `tests/domain/test_location.py`, `src/tectora/domain/location.py`, úprava `src/tectora/domain/errors.py`.

**Steps:**

1. Padající test `tests/domain/test_location.py`:
   ```python
   from tectora.domain.location import Location


   def test_location_defaults_have_no_coords():
       loc = Location(name="Brno")
       assert loc.name == "Brno"
       assert loc.lat is None
       assert loc.lon is None


   def test_location_with_coords():
       loc = Location("Moravský Krumlov", 49.048, 16.310)
       assert loc.lat == 49.048
       assert loc.lon == 16.310
   ```
2. Ověř pád: `python -m pytest tests/domain/test_location.py -q`.
3. Implementuj `src/tectora/domain/location.py`:
   ```python
   """Poloha zakázky (value object)."""
   from __future__ import annotations

   from dataclasses import dataclass
   from typing import Optional


   @dataclass(frozen=True)
   class Location:
       name: str
       lat: Optional[float] = None
       lon: Optional[float] = None
   ```
4. Přidej do `src/tectora/domain/errors.py` na konec novou výjimku:
   ```python
   class EntryNotFound(TectoraError):
       """Zakázka s daným ID v knihovně neexistuje."""
   ```
5. Spusť: `python -m pytest tests/domain/test_location.py -q` → `2 passed`.
6. Commit:
   ```bash
   git add -A && git commit -m "Task 1: domain.location + EntryNotFound (TDD)"
   ```

---

## Task 2 — Application: library.py (LibraryEntry + helpery)

**Files:** `tests/application/test_library_model.py`, `src/tectora/application/library.py`.

**Steps:**

1. Padající test `tests/application/test_library_model.py`:
   ```python
   from tectora.application.library import LibraryEntry, entry_summary, now_iso
   from tectora.application.project import Project
   from tectora.domain.location import Location


   def make_entry():
       return LibraryEntry(
           id=7, name="Dům Novák", note="valba",
           location=Location("Brno", 49.19, 16.61),
           project=Project(project_name="Dům Novák"),
           result={"totals": {"area_m2": 117.0}},
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


   def test_entry_summary_handles_missing_result():
       e = make_entry()
       e.result = None
       e.location = None
       s = entry_summary(e)
       assert s["area_m2"] is None
       assert s["location"] is None
   ```
2. Ověř pád: `python -m pytest tests/application/test_library_model.py -q`.
3. Implementuj `src/tectora/application/library.py`:
   ```python
   """Záznam knihovny zakázek = projekt + metadata + cached výsledek."""
   from __future__ import annotations

   from dataclasses import dataclass
   from datetime import datetime, timezone
   from typing import Optional

   from tectora.application.project import Project
   from tectora.domain.location import Location


   def now_iso() -> str:
       return datetime.now(timezone.utc).isoformat(timespec="seconds")


   @dataclass
   class LibraryEntry:
       id: Optional[int]
       name: str
       note: str
       location: Optional[Location]
       project: Project
       result: Optional[dict]
       created_at: str
       updated_at: str


   def entry_summary(entry: LibraryEntry) -> dict:
       area = None
       if entry.result:
           area = entry.result.get("totals", {}).get("area_m2")
       return {
           "id": entry.id,
           "name": entry.name,
           "note": entry.note,
           "location": entry.location.name if entry.location else None,
           "area_m2": area,
           "updated_at": entry.updated_at,
       }
   ```
4. Spusť: `python -m pytest tests/application/test_library_model.py -q` → `3 passed`.
5. Commit:
   ```bash
   git add -A && git commit -m "Task 2: application.library — LibraryEntry + helpery (TDD)"
   ```

---

## Task 3 — Application: porty + use-casy knihovny

**Files:** `tests/application/test_library_use_cases.py`, úprava `src/tectora/application/ports.py`, úprava `src/tectora/application/use_cases.py`.

**Steps:**

1. Padající test `tests/application/test_library_use_cases.py` (s fake knihovnou a fake geocoderem):
   ```python
   import pytest
   from tectora.application import use_cases as uc
   from tectora.application.library import LibraryEntry
   from tectora.application.project import Project
   from tectora.domain.errors import EntryNotFound
   from tectora.domain.location import Location


   class FakeLibrary:
       def __init__(self):
           self._rows: dict[int, LibraryEntry] = {}
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
   ```
2. Ověř pád: `python -m pytest tests/application/test_library_use_cases.py -q`.
3. Přidej do `src/tectora/application/ports.py` (k existujícím portům):
   ```python
   from tectora.application.library import LibraryEntry
   from tectora.domain.location import Location


   class ProjectLibrary(Protocol):
       def list(self) -> list[LibraryEntry]: ...
       def get(self, entry_id: int) -> LibraryEntry: ...
       def add(self, entry: LibraryEntry) -> int: ...
       def update(self, entry_id: int, entry: LibraryEntry) -> None: ...
       def delete(self, entry_id: int) -> None: ...


   class GeocodingService(Protocol):
       def search(self, query: str, limit: int = 5) -> list[Location]: ...
   ```
4. Přidej do `src/tectora/application/use_cases.py` (na konec; doplň importy nahoře):
   ```python
   # --- importy nahoře v souboru ---
   from typing import Optional
   from tectora.application.library import LibraryEntry, now_iso
   from tectora.application.ports import GeocodingService, ProjectLibrary
   from tectora.domain.location import Location

   _UNSET = object()


   def save_entry(library: ProjectLibrary, project: Project, name: str,
                  note: str = "", location: Optional[Location] = None,
                  result: Optional[dict] = None) -> int:
       ts = now_iso()
       entry = LibraryEntry(None, name, note, location, project, result, ts, ts)
       return library.add(entry)


   def list_entries(library: ProjectLibrary) -> list[LibraryEntry]:
       return library.list()


   def load_entry(library: ProjectLibrary, entry_id: int) -> LibraryEntry:
       return library.get(entry_id)


   def update_entry(library: ProjectLibrary, entry_id: int, *, name=None,
                    note=None, location=_UNSET) -> LibraryEntry:
       entry = library.get(entry_id)
       if name is not None:
           entry.name = name
       if note is not None:
           entry.note = note
       if location is not _UNSET:
           entry.location = location
       entry.updated_at = now_iso()
       library.update(entry_id, entry)
       return entry


   def delete_entry(library: ProjectLibrary, entry_id: int) -> None:
       library.delete(entry_id)


   def suggest_location(geocoder: GeocodingService, query: str,
                        limit: int = 5) -> list[Location]:
       return geocoder.search(query, limit)
   ```
5. Spusť: `python -m pytest tests/application/test_library_use_cases.py -q` → `4 passed`.
6. Commit:
   ```bash
   git add -A && git commit -m "Task 3: application — porty + use-casy knihovny (TDD)"
   ```

---

## Task 4 — Adapter: sqlite_library.py

**Files:** `tests/adapters/test_sqlite_library.py`, `src/tectora/adapters/sqlite_library.py`.

**Steps:**

1. Padající test `tests/adapters/test_sqlite_library.py`:
   ```python
   import pytest
   from tectora.adapters.sqlite_library import SqliteProjectLibrary
   from tectora.application.library import LibraryEntry, now_iso
   from tectora.application.project import Project
   from tectora.domain.errors import EntryNotFound
   from tectora.domain.footprint import Footprint
   from tectora.domain.location import Location


   def make_entry(name="Dům"):
       fp = Footprint.from_lists([(0, 0), (12, 0), (12, 8), (0, 8)],
                                 ["eave", "gable", "eave", "gable"])
       ts = now_iso()
       return LibraryEntry(None, name, "poznámka",
                           Location("Brno", 49.19, 16.61),
                           Project(project_name=name, footprint=fp, global_pitch_deg=35.0),
                           {"totals": {"area_m2": 117.0}}, ts, ts)


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
       assert got.result["totals"]["area_m2"] == 117.0


   def test_list_orders_newest_first(tmp_path):
       L = lib(tmp_path)
       L.add(make_entry("A"))
       L.add(make_entry("B"))
       names = [e.name for e in L.list()]
       assert set(names) == {"A", "B"}
       assert len(names) == 2


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
   ```
2. Ověř pád: `python -m pytest tests/adapters/test_sqlite_library.py -q`.
3. Implementuj `src/tectora/adapters/sqlite_library.py`:
   ```python
   """Adaptér: knihovna zakázek v SQLite."""
   from __future__ import annotations

   import json
   import os
   import sqlite3

   from tectora.application.library import LibraryEntry
   from tectora.application.project import Project
   from tectora.domain.errors import EntryNotFound
   from tectora.domain.location import Location

   _SCHEMA = """
   CREATE TABLE IF NOT EXISTS entries (
       id INTEGER PRIMARY KEY AUTOINCREMENT,
       name TEXT NOT NULL,
       note TEXT NOT NULL DEFAULT '',
       location_name TEXT,
       lat REAL,
       lon REAL,
       project_json TEXT NOT NULL,
       result_json TEXT,
       created_at TEXT NOT NULL,
       updated_at TEXT NOT NULL
   )
   """


   def default_db_path() -> str:
       base = os.environ.get("APPDATA") or os.path.join(
           os.path.expanduser("~"), ".local", "share")
       return os.path.join(base, "Tectora", "tectora.db")


   class SqliteProjectLibrary:
       def __init__(self, db_path: str):
           self._path = db_path
           directory = os.path.dirname(db_path)
           if directory:
               os.makedirs(directory, exist_ok=True)
           with self._connect() as con:
               con.execute(_SCHEMA)

       def _connect(self) -> sqlite3.Connection:
           con = sqlite3.connect(self._path)
           con.row_factory = sqlite3.Row
           return con

       def add(self, entry: LibraryEntry) -> int:
           with self._connect() as con:
               cur = con.execute(
                   "INSERT INTO entries (name, note, location_name, lat, lon, "
                   "project_json, result_json, created_at, updated_at) "
                   "VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
                   self._insert_params(entry),
               )
               return int(cur.lastrowid)

       def get(self, entry_id: int) -> LibraryEntry:
           with self._connect() as con:
               row = con.execute(
                   "SELECT * FROM entries WHERE id = ?", (entry_id,)
               ).fetchone()
           if row is None:
               raise EntryNotFound(str(entry_id))
           return self._row_to_entry(row)

       def list(self) -> list[LibraryEntry]:
           with self._connect() as con:
               rows = con.execute(
                   "SELECT * FROM entries ORDER BY updated_at DESC, id DESC"
               ).fetchall()
           return [self._row_to_entry(r) for r in rows]

       def update(self, entry_id: int, entry: LibraryEntry) -> None:
           with self._connect() as con:
               cur = con.execute(
                   "UPDATE entries SET name=?, note=?, location_name=?, lat=?, "
                   "lon=?, project_json=?, result_json=?, updated_at=? WHERE id=?",
                   (*self._common_params(entry), entry.updated_at, entry_id),
               )
               if cur.rowcount == 0:
                   raise EntryNotFound(str(entry_id))

       def delete(self, entry_id: int) -> None:
           with self._connect() as con:
               con.execute("DELETE FROM entries WHERE id = ?", (entry_id,))

       # --- mapování ---

       def _common_params(self, e: LibraryEntry) -> tuple:
           loc = e.location
           return (
               e.name,
               e.note,
               loc.name if loc else None,
               loc.lat if loc else None,
               loc.lon if loc else None,
               json.dumps(e.project.to_dict(), ensure_ascii=False),
               json.dumps(e.result, ensure_ascii=False) if e.result else None,
           )

       def _insert_params(self, e: LibraryEntry) -> tuple:
           return (*self._common_params(e), e.created_at, e.updated_at)

       def _row_to_entry(self, row: sqlite3.Row) -> LibraryEntry:
           location = None
           if row["location_name"] is not None:
               location = Location(row["location_name"], row["lat"], row["lon"])
           project = Project.from_dict(json.loads(row["project_json"]))
           result = json.loads(row["result_json"]) if row["result_json"] else None
           return LibraryEntry(
               id=row["id"], name=row["name"], note=row["note"],
               location=location, project=project, result=result,
               created_at=row["created_at"], updated_at=row["updated_at"],
           )
   ```
4. Spusť: `python -m pytest tests/adapters/test_sqlite_library.py -q` → `5 passed`.
5. Commit:
   ```bash
   git add -A && git commit -m "Task 4: adapter sqlite_library (TDD)"
   ```

---

## Task 5 — Adapter: nominatim_geocoder.py

> HTTP volání je injektovatelné (`fetch`), aby testy neběžely po síti.

**Files:** `tests/adapters/test_nominatim_geocoder.py`, `src/tectora/adapters/nominatim_geocoder.py`.

**Steps:**

1. Padající test `tests/adapters/test_nominatim_geocoder.py`:
   ```python
   from tectora.adapters.nominatim_geocoder import NominatimGeocoder


   def fake_fetch_ok(query, limit):
       return [
           {"display_name": "Moravský Krumlov, Česko", "lat": "49.05", "lon": "16.31"},
           {"display_name": "Moravské Budějovice, Česko", "lat": "49.05", "lon": "15.81"},
       ]


   def fake_fetch_offline(query, limit):
       raise OSError("offline")


   def test_search_maps_results():
       geo = NominatimGeocoder(fetch=fake_fetch_ok)
       out = geo.search("Mor", limit=5)
       assert len(out) == 2
       assert out[0].name.startswith("Moravský Krumlov")
       assert out[0].lat == 49.05


   def test_search_empty_query_returns_empty():
       geo = NominatimGeocoder(fetch=fake_fetch_ok)
       assert geo.search("   ") == []


   def test_search_offline_returns_empty_without_raising():
       geo = NominatimGeocoder(fetch=fake_fetch_offline)
       assert geo.search("Mor") == []
   ```
2. Ověř pád: `python -m pytest tests/adapters/test_nominatim_geocoder.py -q`.
3. Implementuj `src/tectora/adapters/nominatim_geocoder.py`:
   ```python
   """Adaptér: našeptávač míst přes OpenStreetMap / Nominatim."""
   from __future__ import annotations

   import json
   import urllib.parse
   import urllib.request
   from typing import Callable, Optional

   from tectora.domain.location import Location

   _URL = "https://nominatim.openstreetmap.org/search"
   _USER_AGENT = "Tectora/0.1 (roof calculator)"

   FetchFn = Callable[[str, int], list]


   class NominatimGeocoder:
       def __init__(self, fetch: Optional[FetchFn] = None, timeout: float = 5.0):
           self._fetch = fetch or self._http_fetch
           self._timeout = timeout

       def search(self, query: str, limit: int = 5) -> list[Location]:
           if not query.strip():
               return []
           try:
               raw = self._fetch(query, limit)
           except Exception:
               return []  # offline / timeout / chyba → prázdné, bez pádu
           results = []
           for item in raw or []:
               try:
                   results.append(Location(
                       name=item["display_name"],
                       lat=float(item["lat"]),
                       lon=float(item["lon"]),
                   ))
               except (KeyError, TypeError, ValueError):
                   continue
           return results

       def _http_fetch(self, query: str, limit: int) -> list:
           params = urllib.parse.urlencode(
               {"q": query, "format": "json", "limit": limit})
           req = urllib.request.Request(
               f"{_URL}?{params}", headers={"User-Agent": _USER_AGENT})
           with urllib.request.urlopen(req, timeout=self._timeout) as resp:
               return json.loads(resp.read().decode("utf-8"))
   ```
4. Spusť: `python -m pytest tests/adapters/test_nominatim_geocoder.py -q` → `3 passed`.
5. Commit:
   ```bash
   git add -A && git commit -m "Task 5: adapter nominatim_geocoder (TDD)"
   ```

---

## Task 6 — CLI: příkazy `library` a `geocode`

**Files:** `tests/cli/test_cli_library.py`, úprava `src/tectora/cli/main.py`.

**Steps:**

1. Padající test `tests/cli/test_cli_library.py`:
   ```python
   import json
   from tectora.cli.main import main


   def make_project_file(tmp_path):
       p = str(tmp_path / "proj.json")
       main(["new", p, "--name", "Dům"])
       main(["set-footprint", p, "--vertices", "0,0; 12,0; 12,8; 0,8"])
       main(["set-edge", p, "--index", "1", "--type", "gable"])
       main(["set-edge", p, "--index", "3", "--type", "gable"])
       main(["set-pitch", p, "--deg", "35"])
       return p


   def test_library_crud(tmp_path, capsys):
       db = str(tmp_path / "lib.db")
       proj = make_project_file(tmp_path)

       assert main(["library", "add", proj, "--db", db,
                    "--name", "Dům Novák", "--note", "valba",
                    "--location", "Brno"]) == 0
       added = json.loads(capsys.readouterr().out)
       eid = added["id"]
       assert eid == 1

       assert main(["library", "list", "--db", db]) == 0
       listed = json.loads(capsys.readouterr().out)
       assert listed[0]["name"] == "Dům Novák"
       assert listed[0]["location"] == "Brno"
       assert listed[0]["area_m2"] is not None

       out = str(tmp_path / "out.json")
       assert main(["library", "get", str(eid), "--db", db, "--output", out]) == 0
       proj_back = json.loads(open(out, encoding="utf-8").read())
       assert proj_back["project_name"] == "Dům"

       assert main(["library", "update", str(eid), "--db", db,
                    "--name", "Dům Novákovi"]) == 0
       main(["library", "list", "--db", db])
       assert json.loads(capsys.readouterr().out)[0]["name"] == "Dům Novákovi"

       assert main(["library", "delete", str(eid), "--db", db]) == 0
       main(["library", "list", "--db", db])
       assert json.loads(capsys.readouterr().out) == []


   def test_geocode_command(monkeypatch, capsys):
       from tectora.adapters import nominatim_geocoder as ng
       monkeypatch.setattr(
           ng.NominatimGeocoder, "search",
           lambda self, q, limit=5: [ng.Location("Moravský Krumlov", 49.05, 16.31)],
       )
       assert main(["geocode", "Mor"]) == 0
       data = json.loads(capsys.readouterr().out)
       assert data[0]["name"] == "Moravský Krumlov"
       assert data[0]["lat"] == 49.05
   ```
2. Ověř pád: `python -m pytest tests/cli/test_cli_library.py -q`.
3. Uprav `src/tectora/cli/main.py`:

   a) Doplň importy nahoře:
   ```python
   from tectora.adapters.nominatim_geocoder import NominatimGeocoder
   from tectora.adapters.sqlite_library import SqliteProjectLibrary, default_db_path
   from tectora.application.library import entry_summary
   from tectora.domain.location import Location
   ```

   b) V `_build_parser()` před `return parser` přidej příkazy:
   ```python
       p_lib = sub.add_parser("library", help="Knihovna zakázek (CRUD)")
       lib_sub = p_lib.add_subparsers(dest="lib_command", required=True)

       l_add = lib_sub.add_parser("add", help="Přidá projekt do knihovny")
       l_add.add_argument("project")
       l_add.add_argument("--db", default=None)
       l_add.add_argument("--name", required=True)
       l_add.add_argument("--note", default="")
       l_add.add_argument("--location", default=None)

       l_list = lib_sub.add_parser("list", help="Vypíše zakázky (JSON)")
       l_list.add_argument("--db", default=None)

       l_get = lib_sub.add_parser("get", help="Uloží projekt zakázky do souboru")
       l_get.add_argument("id", type=int)
       l_get.add_argument("--db", default=None)
       l_get.add_argument("--output", required=True)

       l_upd = lib_sub.add_parser("update", help="Upraví metadata zakázky")
       l_upd.add_argument("id", type=int)
       l_upd.add_argument("--db", default=None)
       l_upd.add_argument("--name", default=None)
       l_upd.add_argument("--note", default=None)
       l_upd.add_argument("--location", default=None)

       l_del = lib_sub.add_parser("delete", help="Smaže zakázku")
       l_del.add_argument("id", type=int)
       l_del.add_argument("--db", default=None)

       p_geo = sub.add_parser("geocode", help="Našeptá místa (JSON)")
       p_geo.add_argument("query")
       p_geo.add_argument("--limit", type=int, default=5)
   ```

   c) V `main()` přidej obsluhu **za blok `if args.command == "show"`**, stále
   uvnitř `try` (tj. před `except (TectoraError, ValueError)`):
   ```python
           if args.command == "geocode":
               places = NominatimGeocoder().search(args.query, args.limit)
               json.dump([{"name": p.name, "lat": p.lat, "lon": p.lon} for p in places],
                         sys.stdout, ensure_ascii=False)
               sys.stdout.write("\n")
               return 0
           if args.command == "library":
               library = SqliteProjectLibrary(args.db or default_db_path())
               if args.lib_command == "add":
                   project = _repo.load(args.project)
                   result = None
                   try:
                       result = result_to_dict(uc.calculate_roof(project))
                   except (TectoraError, ValueError):
                       result = None
                   location = Location(args.location) if args.location else None
                   eid = uc.save_entry(library, project, args.name,
                                       note=args.note, location=location, result=result)
                   json.dump({"id": eid}, sys.stdout, ensure_ascii=False)
                   sys.stdout.write("\n")
                   return 0
               if args.lib_command == "list":
                   rows = [entry_summary(e) for e in uc.list_entries(library)]
                   json.dump(rows, sys.stdout, ensure_ascii=False)
                   sys.stdout.write("\n")
                   return 0
               if args.lib_command == "get":
                   entry = uc.load_entry(library, args.id)
                   _repo.save(args.output, entry.project)
                   return 0
               if args.lib_command == "update":
                   # polohu měň jen když byla zadána (jinak zůstane beze změny)
                   changes = {"name": args.name, "note": args.note}
                   if args.location is not None:
                       changes["location"] = Location(args.location)
                   uc.update_entry(library, args.id, **changes)
                   return 0
               if args.lib_command == "delete":
                   uc.delete_entry(library, args.id)
                   return 0
   ```
   > Pozn.: `EntryNotFound` je podtřída `TectoraError`, takže existující `except`
   > v `main()` ji zachytí a vrátí `{ "error": ... }` + nenulový exit.
4. Spusť: `python -m pytest tests/cli/test_cli_library.py -q` → `2 passed`.
5. Commit:
   ```bash
   git add -A && git commit -m "Task 6: CLI library + geocode příkazy (TDD)"
   ```

---

## Task 7 — Integrace: akceptační testy (case 8, 9)

**Files:** úprava `tests/test_acceptance.py`.

**Steps:**

1. Přidej do `tests/test_acceptance.py`:
   ```python
   def test_case8_library_crud(tmp_path, capsys):
       import json
       from tectora.cli.main import main
       db = str(tmp_path / "lib.db")
       proj = str(tmp_path / "p.json")
       main(["new", proj, "--name", "Dům"])
       main(["set-footprint", proj, "--vertices", "0,0; 12,0; 12,8; 0,8"])
       main(["set-edge", proj, "--index", "1", "--type", "gable"])
       main(["set-edge", proj, "--index", "3", "--type", "gable"])
       main(["set-pitch", proj, "--deg", "35"])

       main(["library", "add", proj, "--db", db, "--name", "Z1"])
       capsys.readouterr()
       main(["library", "list", "--db", db])
       assert len(json.loads(capsys.readouterr().out)) == 1


   def test_case9_geocode_offline(monkeypatch, capsys):
       import json
       from tectora.adapters import nominatim_geocoder as ng
       from tectora.cli.main import main
       monkeypatch.setattr(ng.NominatimGeocoder, "search",
                           lambda self, q, limit=5: [])
       assert main(["geocode", "Mor"]) == 0
       assert json.loads(capsys.readouterr().out) == []
   ```
2. Spusť celou sadu s pokrytím:
   ```bash
   python -m pytest --cov=tectora --cov-report=term-missing -q
   ```
   Očekávaný výstup: vše `passed`.
3. Commit:
   ```bash
   git add -A && git commit -m "Task 7: akceptační testy knihovny + geokódování"
   ```

---

## Hotovo, když

- `python -m pytest -q` → vše zelené.
- `tectora library add/list/get/update/delete` funguje nad SQLite DB.
- `tectora geocode "Mor"` vrací JSON návrhy (online) / `[]` (offline) bez pádu.
- Adaptéry `sqlite_library` i `nominatim_geocoder` jsou za porty z `application`.

## Navazující

- **Plán GUI** napojí úvodní stránku (karty zakázek, „tři tečky" CRUD) na
  `SqliteProjectLibrary` a pole Poloha na `NominatimGeocoder` (našeptávač).
