"""Use cases — wrap the existing calculator and ports."""
from __future__ import annotations

from tectora.application.ports import ReportExporter
from tectora.application.project import Project
from tectora.core.bridge import footprint_to_spec
from tectora.core.calculator import calculate
from tectora.core.result import RoofResult


def calculate_project(project: Project) -> RoofResult:
    if project.footprint is None:
        raise ValueError("Projekt nemá zadaný půdorys.")
    spec = footprint_to_spec(
        project.footprint, project.global_pitch_deg, name=project.project_name)
    return calculate(spec)


def export_report(result: RoofResult, exporter: ReportExporter, path: str) -> None:
    exporter.export(result, path)
