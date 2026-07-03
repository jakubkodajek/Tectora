"""Material takeoff (spotřeba krytiny) computed from roof measurements.

Pure domain: turns RoofResult measurements into material line items using a
parametric :class:`MaterialSpec` (consumption rates differ per manufacturer,
so they are inputs with common defaults, not constants). No UI dependency.
"""
from __future__ import annotations

import math
from dataclasses import dataclass
from typing import Iterable

from tectora.core.result import RoofResult


@dataclass(frozen=True)
class MaterialSpec:
    """Consumption rates of the chosen roofing system.

    Defaults are typical for large-format concrete/clay tiles (e.g. ~10 tiles
    per m², ~2.5 ridge caps per metre); adjust to the manufacturer's datasheet.
    """
    tiles_per_m2: float = 10.0       # ks tašek na m² plochy
    waste_pct: float = 5.0           # prořez / rezerva v %
    ridge_caps_per_m: float = 2.5    # ks hřebenáčů na m hřebene + nároží
    gable_tiles_per_m: float = 3.0   # ks krajových tašek na m štítové hrany
    # Linear items (úžlabní pás, okapní plech) are measured 1:1 in metres.


@dataclass(frozen=True)
class MaterialLine:
    """One takeoff row: item name, quantity, unit."""
    name: str
    qty: float
    unit: str


def material_takeoff(
    results: Iterable[RoofResult],
    spec: MaterialSpec = MaterialSpec(),
) -> list[MaterialLine]:
    """Aggregate material quantities for one or more computed roofs.

    Piece counts are rounded UP (you cannot buy 3.2 ridge caps); linear
    metres are reported to two decimals.
    """
    results = list(results)
    area = sum(r.total_area_m2 for r in results)
    ridge_hip = sum(r.ridge_length_m + r.hip_length_m for r in results)
    valley = sum(r.valley_length_m for r in results)
    eaves = sum(r.eaves_perimeter_m for r in results)
    gable = sum(r.gable_edge_total_m for r in results)

    factor = 1.0 + spec.waste_pct / 100.0
    lines = [
        MaterialLine(
            f"Tašky (vč. prořezu {spec.waste_pct:g} %)",
            float(math.ceil(area * spec.tiles_per_m2 * factor)),
            "ks",
        ),
        MaterialLine(
            "Hřebenáče (hřeben + nároží)",
            float(math.ceil(ridge_hip * spec.ridge_caps_per_m)),
            "ks",
        ),
    ]
    if gable > 1e-9:
        lines.append(MaterialLine(
            "Krajové tašky",
            float(math.ceil(gable * spec.gable_tiles_per_m)),
            "ks",
        ))
    if valley > 1e-9:
        lines.append(MaterialLine("Úžlabní pás", round(valley, 2), "m"))
    lines.append(MaterialLine("Okapní plech", round(eaves, 2), "m"))
    return lines


def spec_note(spec: MaterialSpec) -> str:
    """One-line summary of the assumed consumption rates (for the report)."""
    return (
        f"Předpoklady: {spec.tiles_per_m2:g} ks tašek/m², "
        f"prořez {spec.waste_pct:g} %, "
        f"{spec.ridge_caps_per_m:g} ks hřebenáčů/m, "
        f"{spec.gable_tiles_per_m:g} ks krajových tašek/m."
    )
