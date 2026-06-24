"""Adapter: project library stored in SQLite."""
from __future__ import annotations

import json
import os
import sqlite3

from tectora.application.library import EntryNotFound, LibraryEntry
from tectora.application.project import Project
from tectora.core.location import Location

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
