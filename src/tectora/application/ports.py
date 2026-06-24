"""Ports — implemented by adapters."""
from __future__ import annotations

from typing import Protocol

from tectora.application.library import LibraryEntry
from tectora.application.project import Project
from tectora.core.location import Location
from tectora.core.result import RoofResult


class ProjectRepository(Protocol):
    def load(self, path: str) -> Project: ...
    def save(self, path: str, project: Project) -> None: ...


class ReportExporter(Protocol):
    def export(self, result: RoofResult, path: str) -> None: ...


class ProjectLibrary(Protocol):
    def list(self) -> list[LibraryEntry]: ...
    def get(self, entry_id: int) -> LibraryEntry: ...
    def add(self, entry: LibraryEntry) -> int: ...
    def update(self, entry_id: int, entry: LibraryEntry) -> None: ...
    def delete(self, entry_id: int) -> None: ...


class GeocodingService(Protocol):
    def search(self, query: str, limit: int = 5) -> list[Location]: ...
