"""Vstupní specifikace střechy."""
from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum


class RoofType(str, Enum):
    """Podporované typy střech."""
    SHED = "shed"          # pultová
    GABLE = "gable"        # sedlová
    HIP = "hip"            # valbová
    HALF_HIP = "half_hip"  # polovalbová (clipped gable)
    PYRAMID = "pyramid"    # stanová

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
    """Vstupní specifikace pro výpočet střechy.

    Všechny délkové rozměry v metrech, úhly ve stupních.
    `length` a `width` jsou rozměry půdorysu MEZI VENKOVNÍMI HRANAMI ZDÍ.
    Skutečný roof footprint = (length + 2*eave_overhang, width + 2*eave_overhang).
    """
    type: RoofType
    length: float        # L — delší rozměr půdorysu, m
    width: float         # W — kratší rozměr půdorysu, m
    pitch: float         # hlavní sklon, stupně (0–89)
    pitch_back: float | None = None
    """Sklon zadní strany pro asymetrickou sedlovou. None = symetrické (= pitch)."""

    half_hip_ratio: float = 0.5
    """Pouze pro HALF_HIP: 0 → sedlová, 1 → valbová. Délka valby = ratio * W/2."""

    eave_overhang: float = 0.0
    """Přesah okapu od líce zdi, m. Aplikuje se na všech stranách."""

    name: str = ""
    """Volitelný popisek / název projektu."""

    metadata: dict = field(default_factory=dict)

    def __post_init__(self) -> None:
        # Qt comboboxes mohou předat str místo RoofType (RoofType dědí z str).
        # Sjednotíme to vždy na enum, ať `.cs_name` apod. funguje.
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
            # Po přesazení o valbu musí ještě zbýt hřeben
            d = self.half_hip_ratio * (self.roof_width / 2)
            if self.roof_length - 2 * d <= 0:
                raise ValueError(
                    f"hřeben polovalbové vyšel <= 0; "
                    f"length musí být > {2 * d:.3f} m při tomto poměru"
                )

    @property
    def roof_length(self) -> float:
        """Délka půdorysu střechy včetně přesahů okapu."""
        return self.length + 2 * self.eave_overhang

    @property
    def roof_width(self) -> float:
        """Šířka půdorysu střechy včetně přesahů okapu."""
        return self.width + 2 * self.eave_overhang
