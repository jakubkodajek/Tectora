"""Bridge: rectangle Footprint + edge types → RoofSpec (reuses the calculator).

The exact calculator works in its own frame (corner at (0, 0), ridge along x,
"front" face at y = 0). :func:`footprint_to_spec_frame` picks the spec AND the
rigid placement (:class:`CalcFrame`) of that frame onto the drawn rectangle,
based on WHICH edges are eaves:

* GABLE — the ridge runs parallel to the two eave edges, so gables on the
  LONGER sides simply rotate the roof (length may be smaller than width).
* SHED — the single eave edge is the LOW side; the frame flips so the
  calculator's y = 0 lands on that edge.
* HIP / PYRAMID — orientation-symmetric; the ridge follows the longer side.

``generate_roof`` uses the frame to transform the calculator's faces into
model coordinates, so every consumer (2D canvas, 3D view, exports) sees ONE
shared geometry and cannot disagree about orientation.
"""
from __future__ import annotations

from dataclasses import dataclass

from tectora.core.footprint import EdgeType, Footprint
from tectora.core.spec import RoofSpec, RoofType

_TOL = 1e-6


class UnsupportedFootprint(ValueError):
    """A shape the bridge cannot yet convert to RoofSpec (general polygon → skeleton)."""


@dataclass(frozen=True)
class CalcFrame:
    """Rigid placement of the calculator frame in model space.

    ``model = origin + x·ex + y·ey`` where ``ex``/``ey`` are perpendicular
    unit axes from {±(1,0), ±(0,1)} — i.e. an axis-aligned rotation/flip
    plus a translation.
    """
    origin: tuple
    ex: tuple
    ey: tuple

    def to_model(self, x: float, y: float) -> tuple:
        ox, oy = self.origin
        return (
            ox + x * self.ex[0] + y * self.ey[0],
            oy + x * self.ex[1] + y * self.ey[1],
        )


def is_axis_aligned_rectangle(fp: Footprint) -> bool:
    """True iff the footprint is a plain axis-aligned rectangle."""
    if fp.n != 4:
        return False
    xs = sorted({round(v[0], 9) for v in fp.vertices})
    ys = sorted({round(v[1], 9) for v in fp.vertices})
    if len(xs) != 2 or len(ys) != 2:
        return False
    want = {(xs[0], ys[0]), (xs[1], ys[0]), (xs[1], ys[1]), (xs[0], ys[1])}
    return want == {(round(x, 9), round(y, 9)) for x, y in fp.vertices}


def _edge_is_horizontal(fp: Footprint, i: int) -> bool:
    a = fp.vertices[i]
    b = fp.vertices[(i + 1) % fp.n]
    return abs(b[1] - a[1]) < _TOL


def _edge_mid(fp: Footprint, i: int) -> tuple:
    a = fp.vertices[i]
    b = fp.vertices[(i + 1) % fp.n]
    return ((a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0)


def footprint_to_spec_frame(
    fp: Footprint, pitch_deg: float, name: str = "",
) -> tuple[RoofSpec, CalcFrame]:
    """Return the RoofSpec AND the calc→model frame for a rectangle footprint.

    Raises :class:`UnsupportedFootprint` for non-rectangles and for eave/gable
    combinations the exact calculator has no type for (0 eaves, 3 eaves, or
    2 adjacent eaves).
    """
    if not is_axis_aligned_rectangle(fp):
        raise UnsupportedFootprint(
            "Most umí zatím jen obdélník; obecné polygony přijdou se skeletonem.")
    xs = sorted({v[0] for v in fp.vertices})
    ys = sorted({v[1] for v in fp.vertices})
    x0, x1 = xs
    y0, y1 = ys
    w, d = x1 - x0, y1 - y0

    eaves = [i for i, e in enumerate(fp.edges) if e.type is EdgeType.EAVE]
    n = len(eaves)

    # Default frame: ridge along model x, calc origin at the SW corner —
    # matches the historic "longer dimension along x" placement.
    parallel = CalcFrame((x0, y0), (1.0, 0.0), (0.0, 1.0))
    transposed = CalcFrame((x0, y0), (0.0, 1.0), (1.0, 0.0))

    if n == 1:
        # SHED: the single eave is the LOW side → calc y = 0 must land on it.
        i = eaves[0]
        mx, my = _edge_mid(fp, i)
        if _edge_is_horizontal(fp, i):
            length, width = w, d
            if abs(my - y0) < _TOL:
                frame = parallel                                   # low at south
            else:
                frame = CalcFrame((x0, y1), (1.0, 0.0), (0.0, -1.0))  # low at north
        else:
            length, width = d, w
            if abs(mx - x0) < _TOL:
                frame = transposed                                 # low at west
            else:
                frame = CalcFrame((x1, y0), (0.0, 1.0), (-1.0, 0.0))  # low at east
        rtype = RoofType.SHED
    elif n == 2 and (eaves[1] - eaves[0]) == 2:
        # GABLE: ridge parallel to the eave pair. Calc "front" (y = 0) maps to
        # the south / west eave — deterministic, so an asymmetric back pitch
        # keeps meaning the same physical face across recomputes.
        if _edge_is_horizontal(fp, eaves[0]):
            length, width = w, d
            frame = parallel
        else:
            length, width = d, w
            frame = transposed
        rtype = RoofType.GABLE
    elif n == 4 and abs(w - d) < _TOL:
        length, width = w, d
        frame = parallel
        rtype = RoofType.PYRAMID
    elif n == 4:
        # HIP is orientation-symmetric; keep the ridge on the longer side.
        length, width = (w, d) if w >= d else (d, w)
        frame = parallel if w >= d else transposed
        rtype = RoofType.HIP
    else:
        raise UnsupportedFootprint(
            f"Kombinace {n} okapů na obdélníku není podporovaný typ střechy — "
            "podporované: 1 okap (pultová), 2 protilehlé (sedlová), "
            "4 (valbová/stanová).")

    spec = RoofSpec(type=rtype, length=length, width=width, pitch=pitch_deg,
                    name=name)
    return spec, frame


def footprint_to_spec(fp: Footprint, pitch_deg: float, name: str = "") -> RoofSpec:
    spec, _frame = footprint_to_spec_frame(fp, pitch_deg, name=name)
    return spec
