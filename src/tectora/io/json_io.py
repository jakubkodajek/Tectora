"""JSON serialisation and deserialisation for CLI / AI integration."""
from __future__ import annotations

import json
from pathlib import Path
from typing import Any

from tectora.core.result import RoofResult
from tectora.core.spec import RoofSpec, RoofType


def spec_from_dict(d: dict[str, Any]) -> RoofSpec:
    """Lenient parser — accepts both Czech and English keys and aliases."""
    aliases = {
        # type
        "type": "type", "typ": "type",
        # length
        "length": "length", "length_m": "length", "L": "length",
        "delka": "length", "délka": "length",
        # width
        "width": "width", "width_m": "width", "W": "width",
        "sirka": "width", "šířka": "width",
        # pitch
        "pitch": "pitch", "pitch_deg": "pitch", "sklon": "pitch",
        "pitch_back": "pitch_back", "pitch_back_deg": "pitch_back",
        "sklon_zadni": "pitch_back", "sklon_zadní": "pitch_back",
        # half-hip ratio
        "half_hip_ratio": "half_hip_ratio", "valba": "half_hip_ratio",
        "polovalba": "half_hip_ratio",
        # eave overhang
        "eave_overhang": "eave_overhang", "overhang": "eave_overhang",
        "presah": "eave_overhang", "přesah": "eave_overhang",
        # name
        "name": "name", "nazev": "name", "název": "name",
        "project_name": "name",
    }

    normalized = {}
    for key, val in d.items():
        target = aliases.get(key, key)
        normalized[target] = val

    if "type" not in normalized:
        raise ValueError("V JSON specifikaci chybí pole 'type'")

    # Normalise type — accept Czech and English names
    type_map = {
        "shed": RoofType.SHED, "pultova": RoofType.SHED, "pultová": RoofType.SHED,
        "gable": RoofType.GABLE, "sedlova": RoofType.GABLE, "sedlová": RoofType.GABLE,
        "hip": RoofType.HIP, "valbova": RoofType.HIP, "valbová": RoofType.HIP,
        "half_hip": RoofType.HALF_HIP, "polovalbova": RoofType.HALF_HIP,
        "polovalbová": RoofType.HALF_HIP,
        "pyramid": RoofType.PYRAMID, "stanova": RoofType.PYRAMID,
        "stanová": RoofType.PYRAMID,
    }
    raw_type = str(normalized["type"]).lower().strip()
    if raw_type not in type_map:
        raise ValueError(
            f"Neznámý typ střechy: '{normalized['type']}'. "
            f"Povolené: {sorted(set(t.value for t in RoofType))}"
        )
    normalized["type"] = type_map[raw_type]

    return RoofSpec(
        type=normalized["type"],
        length=float(normalized.get("length", 0)),
        width=float(normalized.get("width", 0)),
        pitch=float(normalized.get("pitch", 0)),
        pitch_back=(
            float(normalized["pitch_back"])
            if normalized.get("pitch_back") is not None
            else None
        ),
        half_hip_ratio=float(normalized.get("half_hip_ratio", 0.5)),
        eave_overhang=float(normalized.get("eave_overhang", 0.0)),
        name=str(normalized.get("name", "")),
    )


def read_spec_json(path: Path) -> RoofSpec:
    with open(path, encoding="utf-8") as f:
        data = json.load(f)
    return spec_from_dict(data)


def write_result_json(result: RoofResult, path: Path) -> None:
    with open(path, "w", encoding="utf-8") as f:
        json.dump(result.as_dict(), f, indent=2, ensure_ascii=False)
