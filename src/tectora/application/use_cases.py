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
    """Aggregate result of a multi-building design: per-building results + totals.

    ``overlap_deduction_m2`` is the roof area of LOWER buildings hidden under
    the footprints of HIGHER overlapping buildings (see
    :func:`overlap_deduction`); the total area subtracts it so a tower
    standing on a wing is not billed twice.
    """
    buildings: list[RoofResult] = field(default_factory=list)
    overlap_deduction_m2: float = 0.0

    @property
    def total_area_m2(self) -> float:
        return sum(r.total_area_m2 for r in self.buildings) - self.overlap_deduction_m2

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


def overlap_deduction(items: list) -> float:
    """Roof area of LOWER buildings covered by HIGHER buildings' footprints.

    *items* is a list of ``(footprint_vertices, eave_height_m, RoofResult)``
    triples, one per computed building. For every overlapping pair, the
    building with the LOWER eave loses the part of its roof lying under the
    higher building's plan outline — face by face, each plan intersection
    corrected by that face's slope (steeper faces hide more true area).
    Equal eave heights deduct from the earlier item (deterministic).

    Areas covered by two higher buildings at once are deducted twice —
    acceptable for the compositions the editor produces (one tower per wing).
    """
    import math

    from tectora.core.geom2d import intersection_area, polygon_area

    def _face_plan_deduction(result: RoofResult, upper_poly: list) -> float:
        total = 0.0
        for f in result.faces:
            plan = [(v[0], v[1]) for v in f.vertices]
            if polygon_area(plan) < 1e-9:
                continue  # vertical / degenerate face has no plan footprint
            cut = intersection_area(plan, upper_poly)
            if cut < 1e-9:
                continue
            total += cut / math.cos(math.radians(f.slope_deg))
        return total

    deduction = 0.0
    n = len(items)
    for i in range(n):
        for j in range(n):
            if i == j:
                continue
            fp_low, h_low, res_low = items[i]
            fp_high, h_high, _res_high = items[j]
            lower = h_low < h_high or (h_low == h_high and i < j)
            if not lower:
                continue
            deduction += _face_plan_deduction(res_low, list(fp_high))
    return deduction


def calculate_design(project: Project) -> DesignResult:
    """Compute one RoofResult per building (skipping buildings with no footprint).

    The design total subtracts the overlap deduction: where a higher
    building's footprint covers a lower building's roof, the hidden roof
    area does not exist and must not be summed.
    """
    results: list[RoofResult] = []
    items: list = []
    for b in project.buildings:
        if b.footprint is None:
            continue
        result = generate_roof(b.footprint, b.pitch_deg, name=b.name,
                               overhang=b.overhang_m, half_hip_m=b.half_hip_m,
                               pitch_back=b.pitch_back_deg)
        results.append(result)
        items.append((list(b.footprint.vertices), float(b.eave_height_m), result))
    return DesignResult(results, overlap_deduction_m2=overlap_deduction(items))


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
