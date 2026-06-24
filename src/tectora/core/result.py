"""Output data classes."""
from __future__ import annotations

from dataclasses import dataclass, field
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from tectora.core.spec import RoofSpec


Vertex3D = tuple[float, float, float]


@dataclass
class FaceResult:
    """A single roof face (polygon in 3D space)."""
    name: str
    area_m2: float
    slope_deg: float
    vertices: list[Vertex3D]


@dataclass
class RoofResult:
    """Complete roof calculation result."""
    spec: "RoofSpec"

    total_area_m2: float           # total area of all faces
    ridge_length_m: float          # ridge length
    hip_length_m: float            # total hip rafter length (sum)
    hip_count: int                 # number of hip rafters
    valley_length_m: float         # total valley length
    gable_edge_left_m: float       # edge tile length, left gable
    gable_edge_right_m: float      # edge tile length, right gable
    eaves_perimeter_m: float       # eaves perimeter
    apex_height_m: float           # apex height above eaves

    valley_count: int = 0               # number of valley rafters

    faces: list[FaceResult] = field(default_factory=list)
    notes: list[str] = field(default_factory=list)
    skeleton_segments: list = field(default_factory=list)
    """Each item: (kind, (x1,y1,z1), (x2,y2,z2)) with kind in {ridge,hip,valley}."""

    @property
    def gable_edge_total_m(self) -> float:
        return self.gable_edge_left_m + self.gable_edge_right_m

    def as_dict(self) -> dict:
        """JSON-friendly dict representation for CLI and Excel export."""
        from dataclasses import asdict
        return {
            "project_name": self.spec.name,
            "input": {
                "type": self.spec.type.value,
                "type_cs": self.spec.type.cs_name,
                "length_m": self.spec.length,
                "width_m": self.spec.width,
                "pitch_deg": self.spec.pitch,
                "pitch_back_deg": self.spec.pitch_back,
                "half_hip_ratio": self.spec.half_hip_ratio,
                "eave_overhang_m": self.spec.eave_overhang,
            },
            "results": {
                "total_area_m2": round(self.total_area_m2, 3),
                "ridge_length_m": round(self.ridge_length_m, 3),
                "hip_length_m": round(self.hip_length_m, 3),
                "hip_count": self.hip_count,
                "valley_length_m": round(self.valley_length_m, 3),
                "gable_edge_left_m": round(self.gable_edge_left_m, 3),
                "gable_edge_right_m": round(self.gable_edge_right_m, 3),
                "gable_edge_total_m": round(self.gable_edge_total_m, 3),
                "eaves_perimeter_m": round(self.eaves_perimeter_m, 3),
                "apex_height_m": round(self.apex_height_m, 3),
            },
            "faces": [
                {
                    "name": f.name,
                    "area_m2": round(f.area_m2, 3),
                    "slope_deg": round(f.slope_deg, 2),
                    "vertices": [[round(v, 3) for v in vert] for vert in f.vertices],
                }
                for f in self.faces
            ],
            "notes": list(self.notes),
            "skeleton_segments": [
                {
                    "kind": s[0],
                    "a": [round(c, 6) for c in s[1]],
                    "b": [round(c, 6) for c in s[2]],
                }
                for s in self.skeleton_segments
            ],
        }
