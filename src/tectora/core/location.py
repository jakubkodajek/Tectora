"""Job location (value object)."""
from __future__ import annotations

from dataclasses import dataclass
from typing import Optional


@dataclass(frozen=True)
class Location:
    name: str
    lat: Optional[float] = None
    lon: Optional[float] = None
