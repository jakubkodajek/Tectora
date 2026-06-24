"""Use cases — wrap the existing calculator and ports."""
from __future__ import annotations

from typing import Optional

from tectora.application.library import LibraryEntry, now_iso
from tectora.application.ports import GeocodingService, ProjectLibrary, ReportExporter
from tectora.application.project import Project
from tectora.core.location import Location
from tectora.core.result import RoofResult
from tectora.core.roof_generator import generate_roof


def calculate_project(project: Project) -> RoofResult:
    if project.footprint is None:
        raise ValueError("Projekt nemá zadaný půdorys.")
    return generate_roof(
        project.footprint, project.global_pitch_deg, name=project.project_name)


def export_report(result: RoofResult, exporter: ReportExporter, path: str) -> None:
    exporter.export(result, path)


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


def update_entry_full(library: ProjectLibrary, entry_id: int, *, project=None,
                      name=None, note=None, location=_UNSET, result=None):
    entry = library.get(entry_id)
    if project is not None:
        entry.project = project
    if name is not None:
        entry.name = name
    if note is not None:
        entry.note = note
    if location is not _UNSET:
        entry.location = location
    if result is not None:
        entry.result = result
    entry.updated_at = now_iso()
    library.update(entry_id, entry)
    return entry


def suggest_location(geocoder: GeocodingService, query: str,
                     limit: int = 5) -> list[Location]:
    return geocoder.search(query, limit)
