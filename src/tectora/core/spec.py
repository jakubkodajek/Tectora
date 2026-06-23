"""Input specification for a roof."""
from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum


class RoofType(str, Enum):
    """Supported roof types."""
    SHED = "shed"          # mono-pitch (lean-to)
    GABLE = "gable"        # gable / saddle
    HIP = "hip"            # hip roof
    HALF_HIP = "half_hip"  # half-hip (clipped gable)
    PYRAMID = "pyramid"    # pyramid / tent

    @property
    def cs_name(self) -> str:
        return {
            RoofType.SHED: "pultová",
            RoofType.GABLE: "sedlová",
            RoofType.HIP: "valbová",
            RoofType.HALF_HIP: "polovalbová",
            RoofType.PYRAMID: "stanová",
        }[self]


@dataclass
class RoofSpec:
    """Input specification for roof calculation.

    All length dimensions in metres, angles in degrees.
    `length` and `width` are footprint dimensions BETWEEN OUTER WALL FACES.
    Actual roof footprint = (length + 2*eave_overhang, width + 2*eave_overhang).
    """
    type: RoofType
    length: float        # L — longer footprint dimension, m
    width: float         # W — shorter footprint dimension, m
    pitch: float         # main pitch, degrees (0–89)
    pitch_back: float | None = None
    """Back-face pitch for asymmetric gable. None = symmetric (= pitch)."""

    half_hip_ratio: float = 0.5
    """For HALF_HIP only: 0 → gable, 1 → full hip. Hip length = ratio * W/2."""

    eave_overhang: float = 0.0
    """Eave overhang from wall face, m. Applied on all sides."""

    name: str = ""
    """Optional label / project name."""

    metadata: dict = field(default_factory=dict)

    def __post_init__(self) -> None:
        # Qt combo-boxes may pass a str instead of RoofType (RoofType inherits from str).
        # Always normalise to the enum so `.cs_name` etc. work correctly.
        if not isinstance(self.type, RoofType):
            self.type = RoofType(self.type)

    def validate(self) -> None:
        if self.length <= 0:
            raise ValueError(f"length musí být > 0, je {self.length}")
        if self.width <= 0:
            raise ValueError(f"width musí být > 0, je {self.width}")
        if not (0 < self.pitch < 90):
            raise ValueError(f"pitch musí být v rozsahu (0, 90), je {self.pitch}")
        if self.pitch_back is not None and not (0 < self.pitch_back < 90):
            raise ValueError(f"pitch_back musí být v rozsahu (0, 90), je {self.pitch_back}")
        if not (0.0 <= self.half_hip_ratio <= 1.0):
            raise ValueError(f"half_hip_ratio musí být v <0, 1>, je {self.half_hip_ratio}")
        if self.eave_overhang < 0:
            raise ValueError(f"eave_overhang nesmí být záporný, je {self.eave_overhang}")
        if self.type == RoofType.PYRAMID and abs(self.length - self.width) > 1e-9:
            raise ValueError(
                f"stanová střecha vyžaduje čtvercový půdorys (length == width), "
                f"dostáno {self.length} × {self.width}"
            )
        if self.type == RoofType.HALF_HIP and self.half_hip_ratio > 0:
            # After the hip offset, a positive ridge length must still remain
            d = self.half_hip_ratio * (self.roof_width / 2)
            if self.roof_length - 2 * d <= 0:
                raise ValueError(
                    f"hřeben polovalbové vyšel <= 0; "
                    f"length musí být > {2 * d:.3f} m při tomto poměru"
                )

    @property
    def roof_length(self) -> float:
        """Roof footprint length including eave overhangs."""
        return self.length + 2 * self.eave_overhang

    @property
    def roof_width(self) -> float:
        """Roof footprint width including eave overhangs."""
        return self.width + 2 * self.eave_overhang
