"""Straight-skeleton implementation (Felkel-Obdrzalek style).

SS1 — geometry foundation: LAV (List of Active Vertices) data structure
with per-vertex angle bisectors and convex/reflex detection.

Polygons must be given as a list of (x, y) float tuples in CCW order
(positive signed area).
"""

from __future__ import annotations

from tectora.core import geom2d as g

EPS = g.EPS


# ---------------------------------------------------------------------------
# Vertex
# ---------------------------------------------------------------------------

class Vertex:
    """A node in the circular doubly-linked list of active vertices.

    Attributes
    ----------
    point       : (x, y) position in the polygon plane
    prev        : previous Vertex in the LAV (CCW order)
    next        : next Vertex in the LAV (CCW order)
    edge_left   : unit direction of the incoming edge  prev -> point
    edge_right  : unit direction of the outgoing edge  point -> next
    bisector    : unit interior-bisector direction (points inward)
    is_reflex   : True if this vertex is a reflex (concave) vertex
    """

    __slots__ = ("point", "prev", "next", "edge_left", "edge_right",
                 "bisector", "is_reflex")

    def __init__(self, point: tuple) -> None:
        self.point: tuple = (float(point[0]), float(point[1]))
        self.prev: Vertex | None = None
        self.next: Vertex | None = None
        self.edge_left: tuple | None = None
        self.edge_right: tuple | None = None
        self.bisector: tuple | None = None
        self.is_reflex: bool = False

    def __repr__(self) -> str:  # pragma: no cover
        return f"Vertex({self.point}, reflex={self.is_reflex})"


# ---------------------------------------------------------------------------
# LAV
# ---------------------------------------------------------------------------

class LAV:
    """Circular doubly-linked list of active vertices.

    Attributes
    ----------
    head : the first Vertex inserted (arbitrary starting point)
    """

    def __init__(self, head: Vertex) -> None:
        self.head: Vertex = head

    def vertices(self) -> list[Vertex]:
        """Return all vertices by walking .next until back at head."""
        result = []
        v = self.head
        while True:
            result.append(v)
            v = v.next
            if v is self.head:
                break
        return result


# ---------------------------------------------------------------------------
# Builder
# ---------------------------------------------------------------------------

def _inward_normal(edge_dir: tuple) -> tuple:
    """Return the inward (left) normal of an edge direction in a CCW polygon.

    For a CCW polygon the interior is to the left, so the inward normal is
    obtained by rotating the edge direction 90 degrees counter-clockwise:
    (dx, dy) -> (-dy, dx).
    """
    return (-edge_dir[1], edge_dir[0])


def build_lav(polygon: list[tuple]) -> LAV:
    """Build a LAV from a CCW polygon.

    For each vertex the function computes:
    - edge_left  = normalize(point - prev_point)
    - edge_right = normalize(next_point - point)
    - is_reflex  = cross(edge_left, edge_right) < -EPS
    - bisector   = normalize(normalize(prev - point) + normalize(next - point))
                   (falls back to inward normal of edge_right when collinear)

    Parameters
    ----------
    polygon : list of (x, y) tuples in CCW order

    Returns
    -------
    LAV with head pointing to the first polygon vertex
    """
    n = len(polygon)
    if n < 3:
        raise ValueError("Polygon must have at least 3 vertices.")

    # Create vertex objects
    verts = [Vertex(pt) for pt in polygon]

    # Wire up the circular doubly-linked list
    for i in range(n):
        verts[i].prev = verts[(i - 1) % n]
        verts[i].next = verts[(i + 1) % n]

    # Compute per-vertex geometry
    for v in verts:
        prev_pt = v.prev.point
        curr_pt = v.point
        next_pt = v.next.point

        # Unit edge directions
        v.edge_left = g.normalize(g.sub(curr_pt, prev_pt))
        v.edge_right = g.normalize(g.sub(next_pt, curr_pt))

        # Reflex detection: in a CCW polygon a right turn at the vertex
        # means the interior angle is > 180 degrees (reflex).
        c = g.cross(v.edge_left, v.edge_right)
        v.is_reflex = c < -EPS

        # Interior bisector:
        # The vectors from curr toward prev and toward next, summed and
        # normalised, give the angle bisector pointing into the interior.
        to_prev = g.normalize(g.sub(prev_pt, curr_pt))
        to_next = g.normalize(g.sub(next_pt, curr_pt))
        bis_raw = g.add(to_prev, to_next)

        if g.length(bis_raw) < EPS:
            # Edges are anti-parallel (straight line); fall back to the
            # inward normal of the outgoing edge.
            v.bisector = _inward_normal(v.edge_right)
        else:
            v.bisector = g.normalize(bis_raw)

    return LAV(head=verts[0])
