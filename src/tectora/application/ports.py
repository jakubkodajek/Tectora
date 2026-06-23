"""Porty — implementují adaptéry."""
from __future__ import annotations

from typing import Protocol

from tectora.application.project import Project
from tectora.core.result import RoofResult


class ProjectRepository(Protocol):
    def load(self, path: str) -> Project: ...
    def save(self, path: str, project: Project) -> None: ...


class ReportExporter(Protocol):
    def export(self, result: RoofResult, path: str) -> None: ...
