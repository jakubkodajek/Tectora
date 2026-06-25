"""Design model — a Project is a container of one or more Buildings.

Adopts DACAD's composition model: a design holds multiple parametric roof
bodies (buildings), each an independent footprint + roof parameters computed by
the existing ``core.roof_generator.generate_roof`` engine.

Backward compatibility is mandatory: the legacy single-footprint ``Project``
API (constructor kwargs ``footprint``/``global_pitch_deg``/``overhang_m``/
``wall_height_m`` and the same-named attributes) keeps working and operates on
the first building. Serialization writes schema 2.0 (``buildings`` array) and
reads both 1.0 (top-level single ``footprint``) and 2.0.
"""
from __future__ import annotations

from dataclasses import dataclass
from typing import Optional

from tectora.core.footprint import Footprint

SCHEMA_VERSION = "2.0"


@dataclass
class Building:
    """One roof body: a footprint plus its roof parameters."""
    footprint: Optional[Footprint] = None
    pitch_deg: float = 35.0
    overhang_m: float = 0.0
    wall_height_m: float = 2.5
    half_hip_m: float = 0.0      # half-hip (polovalba) clip depth in metres
    name: str = ""

    def to_dict(self) -> dict:
        fp = None
        if self.footprint is not None:
            fp = {
                "vertices": [[x, y] for x, y in self.footprint.vertices],
                "edges": [{"type": e.type.value} for e in self.footprint.edges],
            }
        return {
            "name": self.name,
            "footprint": fp,
            "pitch_deg": self.pitch_deg,
            "overhang_m": self.overhang_m,
            "wall_height_m": self.wall_height_m,
            "half_hip_m": self.half_hip_m,
        }

    @classmethod
    def from_dict(cls, data: dict) -> "Building":
        fp = None
        if data.get("footprint"):
            fp = Footprint.from_lists(
                [(v[0], v[1]) for v in data["footprint"]["vertices"]],
                [e["type"] for e in data["footprint"]["edges"]],
            )
        return cls(
            footprint=fp,
            pitch_deg=float(data.get("pitch_deg", 35.0)),
            overhang_m=float(data.get("overhang_m", 0.0)),
            wall_height_m=float(data.get("wall_height_m", 2.5)),
            half_hip_m=float(data.get("half_hip_m", 0.0)),
            name=data.get("name", ""),
        )


class Project:
    """A design = project metadata + a list of :class:`Building` bodies.

    The legacy single-building API is preserved: constructing with a
    ``footprint`` (and no ``buildings``) creates one building, and the
    ``footprint``/``global_pitch_deg``/``overhang_m``/``wall_height_m``
    properties read/write ``buildings[0]``.
    """

    def __init__(
        self,
        project_name: str = "",
        footprint: Optional[Footprint] = None,
        global_pitch_deg: float = 35.0,
        overhang_m: float = 0.0,
        units: str = "m",
        wall_height_m: float = 2.5,
        buildings: Optional[list[Building]] = None,
    ):
        self.project_name = project_name
        self.units = units
        if buildings is not None:
            self.buildings = list(buildings)
        else:
            # Legacy single-building shape. Always keep at least one building so
            # the legacy properties have something to delegate to (its footprint
            # may be None for a not-yet-drawn project).
            self.buildings = [
                Building(
                    footprint=footprint,
                    pitch_deg=global_pitch_deg,
                    overhang_m=overhang_m,
                    wall_height_m=wall_height_m,
                )
            ]

    # ------------------------------------------------------------------
    # Legacy single-building properties (delegate to buildings[0])
    # ------------------------------------------------------------------

    def _primary(self) -> Building:
        if not self.buildings:
            self.buildings = [Building()]
        return self.buildings[0]

    @property
    def footprint(self) -> Optional[Footprint]:
        return self._primary().footprint

    @footprint.setter
    def footprint(self, value: Optional[Footprint]) -> None:
        self._primary().footprint = value

    @property
    def global_pitch_deg(self) -> float:
        return self._primary().pitch_deg

    @global_pitch_deg.setter
    def global_pitch_deg(self, value: float) -> None:
        self._primary().pitch_deg = float(value)

    @property
    def overhang_m(self) -> float:
        return self._primary().overhang_m

    @overhang_m.setter
    def overhang_m(self, value: float) -> None:
        self._primary().overhang_m = float(value)

    @property
    def wall_height_m(self) -> float:
        return self._primary().wall_height_m

    @wall_height_m.setter
    def wall_height_m(self, value: float) -> None:
        self._primary().wall_height_m = float(value)

    # ------------------------------------------------------------------
    # Serialization
    # ------------------------------------------------------------------

    def to_dict(self) -> dict:
        return {
            "schema_version": SCHEMA_VERSION,
            "project_name": self.project_name,
            "units": self.units,
            "buildings": [b.to_dict() for b in self.buildings],
        }

    @classmethod
    def from_dict(cls, data: dict) -> "Project":
        if data.get("buildings") is not None:
            # Schema 2.0
            buildings = [Building.from_dict(b) for b in data["buildings"]]
            if not buildings:
                buildings = [Building()]
            return cls(
                project_name=data.get("project_name", ""),
                units=data.get("units", "m"),
                buildings=buildings,
            )
        # Schema 1.0 — a single top-level footprint becomes one building.
        building = Building.from_dict({
            "footprint": data.get("footprint"),
            "pitch_deg": data.get("global_pitch_deg", 35.0),
            "overhang_m": data.get("overhang_m", 0.0),
            "wall_height_m": data.get("wall_height_m", 2.5),
        })
        return cls(
            project_name=data.get("project_name", ""),
            units=data.get("units", "m"),
            buildings=[building],
        )
