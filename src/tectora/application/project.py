"""Projekt = Footprint + globální sklon + metadata. Mapuje na JSON v1.0."""
from __future__ import annotations

from dataclasses import dataclass
from typing import Optional

from tectora.core.footprint import Footprint

SCHEMA_VERSION = "1.0"


@dataclass
class Project:
    project_name: str = ""
    footprint: Optional[Footprint] = None
    global_pitch_deg: float = 35.0
    overhang_m: float = 0.0
    units: str = "m"

    def to_dict(self) -> dict:
        fp = None
        if self.footprint is not None:
            fp = {
                "vertices": [[x, y] for x, y in self.footprint.vertices],
                "edges": [{"type": e.type.value} for e in self.footprint.edges],
            }
        return {
            "schema_version": SCHEMA_VERSION,
            "project_name": self.project_name,
            "units": self.units,
            "footprint": fp,
            "global_pitch_deg": self.global_pitch_deg,
            "overhang_m": self.overhang_m,
        }

    @classmethod
    def from_dict(cls, data: dict) -> "Project":
        fp = None
        if data.get("footprint"):
            fp = Footprint.from_lists(
                [(v[0], v[1]) for v in data["footprint"]["vertices"]],
                [e["type"] for e in data["footprint"]["edges"]],
            )
        return cls(
            project_name=data.get("project_name", ""),
            footprint=fp,
            global_pitch_deg=float(data.get("global_pitch_deg", 35.0)),
            overhang_m=float(data.get("overhang_m", 0.0)),
            units=data.get("units", "m"),
        )
