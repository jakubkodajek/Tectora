"""Footprint as a polygon with typed edges (eave/gable). Pure domain, no UI."""
from __future__ import annotations

import math
from dataclasses import dataclass
from enum import Enum

Point = tuple[float, float]


class EdgeType(str, Enum):
    EAVE = "eave"
    GABLE = "gable"


@dataclass(frozen=True)
class Edge:
    type: EdgeType


def _signed_area(pts: list[Point]) -> float:
    n = len(pts)
    return 0.5 * sum(pts[i][0] * pts[(i + 1) % n][1] - pts[(i + 1) % n][0] * pts[i][1]
                     for i in range(n))


def _seg_intersect(a, b, c, d) -> bool:
    def orient(p, q, r):
        v = (q[1] - p[1]) * (r[0] - q[0]) - (q[0] - p[0]) * (r[1] - q[1])
        return 0 if abs(v) < 1e-12 else (1 if v > 0 else -1)
    o1, o2, o3, o4 = orient(a, b, c), orient(a, b, d), orient(c, d, a), orient(c, d, b)
    return o1 != o2 and o3 != o4


def _is_simple(pts: list[Point]) -> bool:
    n = len(pts)
    for i in range(n):
        for j in range(i + 1, n):
            if j == i or (j + 1) % n == i or (i + 1) % n == j:
                continue
            if _seg_intersect(pts[i], pts[(i + 1) % n], pts[j], pts[(j + 1) % n]):
                return False
    return True


@dataclass(frozen=True)
class Footprint:
    vertices: tuple[Point, ...]
    edges: tuple[Edge, ...]

    @classmethod
    def from_lists(cls, vertices: list[Point], edge_types: list[str]) -> "Footprint":
        pts = [(float(x), float(y)) for x, y in vertices]
        if len(pts) < 3:
            raise ValueError("Půdorys musí mít alespoň 3 body.")
        if len(edge_types) != len(pts):
            raise ValueError("Počet hran musí odpovídat počtu bodů.")
        try:
            edges = tuple(Edge(EdgeType(t)) for t in edge_types)
        except ValueError as exc:
            raise ValueError(f"Neplatný typ hrany: {exc}") from exc
        if not _is_simple(pts):
            raise ValueError("Půdorys se sám protíná.")
        if abs(_signed_area(pts)) < 1e-9:
            raise ValueError("Půdorys má nulovou plochu.")
        if _signed_area(pts) < 0:  # normalise to CCW winding
            pts = list(reversed(pts))
            rev = list(reversed(edges))
            edges = tuple(rev[1:] + rev[:1])
        return cls(tuple(pts), edges)

    @property
    def n(self) -> int:
        return len(self.edges)

    def edge_vector(self, i: int) -> Point:
        a, b = self.vertices[i], self.vertices[(i + 1) % self.n]
        return (b[0] - a[0], b[1] - a[1])

    def edge_length(self, i: int) -> float:
        dx, dy = self.edge_vector(i)
        return math.hypot(dx, dy)
