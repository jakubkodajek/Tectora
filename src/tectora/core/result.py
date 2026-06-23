"""Výstupní datové třídy."""
from __future__ import annotations

from dataclasses import dataclass, field
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from tectora.core.spec import RoofSpec


Vertex3D = tuple[float, float, float]


@dataclass
class FaceResult:
    """Jedna rovina střechy (poly v 3D)."""
    name: str
    area_m2: float
    slope_deg: float
    vertices: list[Vertex3D]


@dataclass
class RoofResult:
    """Kompletní výsledek výpočtu střechy."""
    spec: "RoofSpec"

    total_area_m2: float           # celková plocha všech rovin
    ridge_length_m: float          # délka hřebene
    hip_length_m: float            # celková délka nároží (součet)
    hip_count: int                 # počet nároží
    valley_length_m: float         # celková délka úžlabí
    gable_edge_left_m: float       # délka krajových tašek levý štít
    gable_edge_right_m: float      # délka krajových tašek pravý štít
    eaves_perimeter_m: float       # obvod okapové hrany
    apex_height_m: float           # výška hřebene nad okapem

    faces: list[FaceResult] = field(default_factory=list)
    notes: list[str] = field(default_factory=list)

    @property
    def gable_edge_total_m(self) -> float:
        return self.gable_edge_left_m + self.gable_edge_right_m

    def as_dict(self) -> dict:
        """JSON-friendly dict reprezentace pro CLI a Excel export."""
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
        }
