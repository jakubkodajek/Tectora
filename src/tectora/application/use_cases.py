"""Use cases — wrap the existing calculator and ports."""
from __future__ import annotations

from dataclasses import dataclass, field
from typing import Optional

from tectora.application.library import LibraryEntry, now_iso
from tectora.application.ports import GeocodingService, ProjectLibrary, ReportExporter
from tectora.application.project import Building, Project
from tectora.core.location import Location
from tectora.core.result import RoofResult
from tectora.core.roof_generator import generate_roof


def calculate_project(project: Project) -> RoofResult:
    if project.footprint is None:
        raise ValueError("Projekt nemá zadaný půdorys.")
    b0 = project.buildings[0] if project.buildings else None
    half_hip = b0.half_hip_m if b0 else 0.0
    pitch_back = b0.pitch_back_deg if b0 else None
    return generate_roof(
        project.footprint, project.global_pitch_deg, name=project.project_name,
        overhang=project.overhang_m, half_hip_m=half_hip, pitch_back=pitch_back,
    )


@dataclass
class DesignResult:
    """Aggregate result of a multi-building design: per-building results + totals."""
    buildings: list[RoofResult] = field(default_factory=list)

    @property
    def total_area_m2(self) -> float:
        return sum(r.total_area_m2 for r in self.buildings)

    @property
    def total_eaves_perimeter_m(self) -> float:
        return sum(r.eaves_perimeter_m for r in self.buildings)

    @property
    def total_ridge_length_m(self) -> float:
        return sum(r.ridge_length_m for r in self.buildings)

    @property
    def total_hip_length_m(self) -> float:
        return sum(r.hip_length_m for r in self.buildings)

    @property
    def total_valley_length_m(self) -> float:
        return sum(r.valley_length_m for r in self.buildings)

    @property
    def total_gable_edge_m(self) -> float:
        return sum(r.gable_edge_total_m for r in self.buildings)


def calculate_design(project: Project) -> DesignResult:
    """Compute one RoofResult per building (skipping buildings with no footprint)."""
    results: list[RoofResult] = []
    for b in project.buildings:
        if b.footprint is None:
            continue
        results.append(
            generate_roof(b.footprint, b.pitch_deg, name=b.name,
                          overhang=b.overhang_m, half_hip_m=b.half_hip_m,
                          pitch_back=b.pitch_back_deg)
        )
    return DesignResult(results)


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
