"""Library entry = project + metadata + cached calculation result."""
from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime, timezone
from typing import Optional

from tectora.application.project import Project
from tectora.core.location import Location


class EntryNotFound(Exception):
    """No library entry exists with the given id."""


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
        area = entry.result.get("results", {}).get("total_area_m2")
    return {
        "id": entry.id,
        "name": entry.name,
        "note": entry.note,
        "location": entry.location.name if entry.location else None,
        "area_m2": area,
        "updated_at": entry.updated_at,
    }
