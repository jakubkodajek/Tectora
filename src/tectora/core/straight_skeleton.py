"""Straight-skeleton implementation (Felkel-Obdrzalek style).

SS1 — geometry foundation: LAV (List of Active Vertices) data structure
with per-vertex angle bisectors and convex/reflex detection.

SS2 — edge-event simulation for convex polygons.

Polygons must be given as a list of (x, y) float tuples in CCW order
(positive signed area).
"""

from __future__ import annotations

import heapq

from tectora.core import geom2d as g

EPS = g.EPS


# ---------------------------------------------------------------------------
# Exceptions
# ---------------------------------------------------------------------------

class RoofGenerationError(Exception):
    """Raised when the straight-skeleton simulation fails to converge."""


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
    active      : False once this vertex has been merged/removed
    """

    __slots__ = ("point", "prev", "next", "edge_left", "edge_right",
                 "bisector", "is_reflex", "active", "t")

    def __init__(self, point: tuple) -> None:
        self.point: tuple = (float(point[0]), float(point[1]))
        self.prev: Vertex | None = None
        self.next: Vertex | None = None
        self.edge_left: tuple | None = None
        self.edge_right: tuple | None = None
        self.bisector: tuple | None = None
        self.is_reflex: bool = False
        self.active: bool = True
        self.t: float = 0.0  # offset time at which this vertex was created

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


# ---------------------------------------------------------------------------
# SkeletonGraph — output data structure
# ---------------------------------------------------------------------------

class SkeletonGraph:
    """The straight-skeleton result.

    Attributes
    ----------
    nodes : list of (x, y, t) interior skeleton nodes with offset height *t*
    arcs  : list of ((x1, y1), (x2, y2)) skeleton arc segments
    """

    def __init__(self) -> None:
        self.nodes: list[tuple] = []   # (x, y, t)
        self.arcs: list[tuple] = []    # ((x1, y1), (x2, y2))

    def _add_node(self, pt: tuple, t: float) -> None:
        self.nodes.append((pt[0], pt[1], t))

    def _add_arc(self, a: tuple, b: tuple) -> None:
        self.arcs.append((a, b))


# ---------------------------------------------------------------------------
# Bisector helper (reusable for merged vertices)
# ---------------------------------------------------------------------------

def _compute_bisector(prev_pt: tuple, curr_pt: tuple, next_pt: tuple,
                      edge_left: tuple, edge_right: tuple) -> tuple:
    """Compute the interior bisector for a vertex given its neighbour points."""
    to_prev = g.normalize(g.sub(prev_pt, curr_pt))
    to_next = g.normalize(g.sub(next_pt, curr_pt))
    bis_raw = g.add(to_prev, to_next)
    if g.length(bis_raw) < EPS:
        return _inward_normal(edge_right)
    return g.normalize(bis_raw)


# ---------------------------------------------------------------------------
# Event helpers
# ---------------------------------------------------------------------------

def _edge_event_candidate(v: "Vertex", vnext: "Vertex"):
    """Compute the edge-event candidate (t, I) for edge v -> vnext.

    Returns (t, I) or None if the bisectors are parallel or degenerate.
    t  = absolute offset time (v.t + perpendicular advance from v's right edge)
    I  = intersection point of the two bisector rays
    """
    I = g.line_intersection(v.point, v.bisector, vnext.point, vnext.bisector)
    if I is None:
        return None
    # Compute the perpendicular advance from v's position toward I
    # along the inward normal of v's right polygon edge.
    # inward normal of CCW polygon edge (dx,dy) is (-dy, dx).
    er = v.edge_right
    inward_n = (-er[1], er[0])
    advance = g.dot(g.sub(I, v.point), inward_n)
    t = v.t + advance
    if t < v.t - EPS:
        return None
    t = max(t, v.t)
    # Skip degenerate events where I has not advanced beyond v's current time
    if abs(t - v.t) < EPS and abs(t - vnext.t) < EPS:
        return None
    return (t, I)


def _push_edge_event(heap, counter, v: "Vertex", vnext: "Vertex") -> None:
    """Push an edge-event onto the min-heap if it exists."""
    result = _edge_event_candidate(v, vnext)
    if result is None:
        return
    t, I = result
    # Use counter as tie-breaker to avoid comparing Vertex objects
    heapq.heappush(heap, (t, next(counter), I, v, vnext))


# ---------------------------------------------------------------------------
# Active-ring traversal (only active nodes, using rewired .next/.prev links)
# ---------------------------------------------------------------------------

def _walk_active_ring(start: "Vertex") -> list["Vertex"]:
    """Return all active vertices in ring order starting from *start*.

    Uses the *rewired* .next/.prev pointers (skipping inactive nodes is done
    by the caller when wiring M into the ring).  This function assumes that
    the .next chain only visits active vertices.
    """
    result = [start]
    v = start.next
    while v is not start:
        result.append(v)
        v = v.next
        if len(result) > 10_000:  # safety valve
            break
    return result


# ---------------------------------------------------------------------------
# skeletonize
# ---------------------------------------------------------------------------

def skeletonize(polygon_ccw: list[tuple]) -> SkeletonGraph:
    """Compute the straight skeleton of a simple convex CCW polygon.

    Parameters
    ----------
    polygon_ccw : list of (x, y) tuples in CCW order (no repeated last vertex)

    Returns
    -------
    SkeletonGraph with .nodes and .arcs

    Raises
    ------
    RoofGenerationError  if iteration limit exceeded
    ValueError           if polygon has fewer than 3 vertices
    """
    lav = build_lav(polygon_ccw)
    sg = SkeletonGraph()

    n = len(polygon_ccw)
    max_iter = 10 * n

    import itertools
    _counter = itertools.count()

    # --- Build initial event heap ---
    heap: list = []
    verts = lav.vertices()
    n_active = len(verts)  # track separately
    for v in verts:
        _push_edge_event(heap, _counter, v, v.next)

    iterations = 0

    while heap:
        iterations += 1
        if iterations > max_iter:
            raise RoofGenerationError(
                f"Straight-skeleton simulation exceeded {max_iter} iterations."
            )

        t, _seq, I, V, Vnext = heapq.heappop(heap)

        # Skip stale events: either vertex was already merged
        if not V.active or not Vnext.active:
            continue
        # Skip if no longer adjacent (the ring was rewired around them)
        if V.next is not Vnext:
            continue

        if n_active == 3:
            # Triangle tail: three vertices meet at one final node.
            # V and Vnext are two of them; the third is V.prev (= Vnext.next).
            third = V.prev  # in the active ring, V.prev == Vnext.next

            # The collapse point is I (intersection of V and Vnext bisectors).
            # If third is already at I (simultaneous collapse from a prior merge),
            # reuse the existing node rather than creating a duplicate.
            third_at_I = (
                abs(third.point[0] - I[0]) < 1e-6 and
                abs(third.point[1] - I[1]) < 1e-6
            )

            if not third_at_I:
                # Genuine triangle collapse — new node.
                sg._add_node(I, t)
                sg._add_arc(V.point, I)
                sg._add_arc(Vnext.point, I)
                sg._add_arc(third.point, I)
            else:
                # Simultaneous collapse: third is already the merged node at I.
                # Just add arcs for V and Vnext; third's arc was added when it
                # was created by the prior normal event.
                sg._add_arc(V.point, I)
                sg._add_arc(Vnext.point, I)

            V.active = False
            Vnext.active = False
            third.active = False
            n_active = 0
            break

        # --- Normal edge-event: merge V and Vnext into new vertex M ---
        sg._add_node(I, t)
        sg._add_arc(V.point, I)
        sg._add_arc(Vnext.point, I)

        # Neighbours in the active ring
        prev_v = V.prev    # already active (ring is maintained)
        next_vn = Vnext.next  # already active

        # Create merged vertex M at I (inherits offset time t)
        M = Vertex(I)
        M.t = t
        M.edge_left = V.edge_left
        M.edge_right = Vnext.edge_right

        # Wire M into the active ring
        M.prev = prev_v
        M.next = next_vn
        prev_v.next = M
        next_vn.prev = M

        # Compute bisector for M from its new neighbours
        M.bisector = _compute_bisector(
            prev_v.point, M.point, next_vn.point,
            M.edge_left, M.edge_right
        )
        c = g.cross(M.edge_left, M.edge_right)
        M.is_reflex = c < -EPS

        # Deactivate the two merged vertices
        V.active = False
        Vnext.active = False
        n_active -= 1  # two removed, one added → net -1

        # Update LAV head to M if head was removed
        if lav.head is V or lav.head is Vnext:
            lav.head = M

        # Push new edge events involving M
        _push_edge_event(heap, _counter, M.prev, M)
        _push_edge_event(heap, _counter, M, M.next)

    return sg
