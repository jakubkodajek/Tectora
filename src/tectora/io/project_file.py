"""Portable project file (.tectora = JSON) for sharing drawings between users."""
from __future__ import annotations

import json
from typing import Optional, Tuple

from tectora.application.project import Project
from tectora.core.location import Location


def save_project_file(path: str, project: Project, note: str = "",
                      location: Optional[Location] = None) -> None:
    data = project.to_dict()
    data["note"] = note
    data["location"] = (
        {"name": location.name, "lat": location.lat, "lon": location.lon}
        if location is not None else None
    )
    with open(path, "w", encoding="utf-8") as fh:
        json.dump(data, fh, ensure_ascii=False, indent=2)


def load_project_file(path: str) -> Tuple[Project, str, Optional[Location]]:
    with open(path, encoding="utf-8") as fh:
        data = json.load(fh)
    project = Project.from_dict(data)
    note = data.get("note", "") or ""
    loc_data = data.get("location")
    location = (
        Location(loc_data["name"], loc_data.get("lat"), loc_data.get("lon"))
        if loc_data else None
    )
    return project, note, location
