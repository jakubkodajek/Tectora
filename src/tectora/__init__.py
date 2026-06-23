"""Tectora — kalkulátor střech."""

from tectora.core.spec import RoofSpec, RoofType
from tectora.core.result import RoofResult, FaceResult
from tectora.core.calculator import calculate

__version__ = "0.1.0"
__all__ = ["RoofSpec", "RoofType", "RoofResult", "FaceResult", "calculate"]
