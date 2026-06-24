"""Straight-skeleton implementation (Felkel-Obdrzalek style).

SS1 — geometry foundation: LAV (List of Active Vertices) data structure
with per-vertex angle bisectors and convex/reflex detection.

SS2 — edge-event simulation for convex polygons.

SS3 — split events for reflex vertices (valleys).

Polygons must be given as a list of (x, y) float tuples in CCW order
(positive signed area).
"""

from __future__ import annotations

import itertools
import heapq

from tectora.core import geom2d as g

EPS = g.EPS

# Event type tags (used as heap tie-breaker discriminator)
_EDGE_EVENT = 0
_SPLIT_EVENT = 1


# ---------------------------------------------------------------------------
# Exceptions
# ---------------------------------------------------------------------------

class RoofGenerationError(ValueError):
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

    def size(self) -> int:
        return len(self.vertices())


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
    - bisector   = interior bisector direction (points INTO the polygon)
                   For convex vertices: normalize(to_prev + to_next)
                   For reflex vertices: negate the above (exterior→interior flip)

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
        # normalised, give the angle bisector.
        #
        # For convex vertices this bisector already points INTO the polygon.
        # For reflex vertices (interior angle > 180°) the sum-of-unit-vectors
        # formula yields the EXTERIOR bisector, so we negate it.
        to_prev = g.normalize(g.sub(prev_pt, curr_pt))
        to_next = g.normalize(g.sub(next_pt, curr_pt))
        bis_raw = g.add(to_prev, to_next)

        if g.length(bis_raw) < EPS:
            # Edges are anti-parallel (straight line); use inward normal.
            bis_unit = _inward_normal(v.edge_right)
        else:
            bis_unit = g.normalize(bis_raw)
            if v.is_reflex:
                # Flip to interior direction
                bis_unit = (-bis_unit[0], -bis_unit[1])

        v.bisector = bis_unit

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
        # Skip degenerate zero-length arcs
        if abs(a[0] - b[0]) < 1e-9 and abs(a[1] - b[1]) < 1e-9:
            return
        self.arcs.append((a, b))


# ---------------------------------------------------------------------------
# Bisector helper (reusable for merged vertices)
# ---------------------------------------------------------------------------

def _compute_bisector(prev_pt: tuple, curr_pt: tuple, next_pt: tuple,
                      edge_left: tuple, edge_right: tuple,
                      is_reflex: bool = False) -> tuple:
    """Compute the interior bisector for a vertex given its neighbour points.

    Handles degenerate cases where prev_pt == curr_pt or next_pt == curr_pt
    by falling back to the inward normal of the available edge.
    """
    v_to_prev = g.sub(prev_pt, curr_pt)
    v_to_next = g.sub(next_pt, curr_pt)

    len_prev = g.length(v_to_prev)
    len_next = g.length(v_to_next)

    if len_prev < EPS and len_next < EPS:
        # Both neighbours coincide with curr — extreme degenerate; use edge_right normal
        return _inward_normal(edge_right)
    elif len_prev < EPS:
        # prev coincides with curr; use inward normal of left edge
        return _inward_normal(edge_left)
    elif len_next < EPS:
        # next coincides with curr; use inward normal of right edge
        return _inward_normal(edge_right)

    to_prev = (v_to_prev[0] / len_prev, v_to_prev[1] / len_prev)
    to_next = (v_to_next[0] / len_next, v_to_next[1] / len_next)
    bis_raw = g.add(to_prev, to_next)

    if g.length(bis_raw) < EPS:
        bis_unit = _inward_normal(edge_right)
    else:
        bis_unit = g.normalize(bis_raw)
        if is_reflex:
            bis_unit = (-bis_unit[0], -bis_unit[1])

    return bis_unit


# ---------------------------------------------------------------------------
# Velocity helper
# ---------------------------------------------------------------------------

def _vertex_velocity(v: "Vertex") -> tuple:
    """Return the actual 2-D velocity of vertex v.

    The velocity is in the bisector direction, scaled so that the
    perpendicular advance toward the right edge is exactly 1 per unit time.

    v_vel = v.bisector / dot(v.bisector, n_right)

    where n_right = (-v.edge_right[1], v.edge_right[0]) is the inward normal
    of the outgoing edge.
    """
    er = v.edge_right
    n_right = (-er[1], er[0])
    denom = g.dot(v.bisector, n_right)
    if abs(denom) < EPS:
        # Bisector is parallel to the edge; return a large velocity (no advance)
        # This vertex effectively doesn't produce valid split events.
        return (0.0, 0.0)
    s = 1.0 / denom
    return (v.bisector[0] * s, v.bisector[1] * s)


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
    # (_EDGE_EVENT, t, seq, I, v, vnext_or_None)
    heapq.heappush(heap, (t, _EDGE_EVENT, next(counter), I, v, vnext))


def _split_event_candidate(v_reflex: "Vertex", edge_a: "Vertex"):
    """Compute the split-event candidate for reflex vertex v_reflex against
    the wavefront edge (edge_a → edge_a.next).

    Returns (t, B) or None.

    The method: find time τ when v_reflex's moving position lies on the
    moving line of the edge edge_a→edge_a.next, with B strictly inside
    the moving edge extent.

    The moving line at time τ has equation: dot(P - A_k, n_k) = τ
    where A_k = edge_a.point (original) and n_k is the inward unit normal.

    The reflex vertex position at absolute time τ (started at v_reflex.t):
      P(τ) = v_reflex.point + (τ - v_reflex.t) * vel

    Substituting and solving:
      dot(v_reflex.point - A_k, n_k) + (τ - v_reflex.t) * dot(vel, n_k) = τ
      Let d = dot(v_reflex.point - A_k, n_k)
      d - v_reflex.t * dot(vel, n_k) + τ * dot(vel, n_k) = τ
      d - v_reflex.t * dot(vel, n_k) = τ * (1 - dot(vel, n_k))
      τ = (d - v_reflex.t * dot(vel, n_k)) / (1 - dot(vel, n_k))

    After finding τ, compute B = P(τ) and verify B is strictly inside the
    moving extent of the edge.
    """
    edge_b = edge_a.next

    # Inward unit normal of the original edge edge_a→edge_b
    er = edge_a.edge_right  # unit direction of this edge
    n_k = (-er[1], er[0])   # inward normal

    # Actual velocity of the reflex vertex
    vel = _vertex_velocity(v_reflex)
    if vel == (0.0, 0.0):
        return None

    dot_vel_nk = g.dot(vel, n_k)
    denom = 1.0 - dot_vel_nk

    if abs(denom) < EPS:
        # Vertex moves parallel to the edge; no split event
        return None

    d = g.dot(g.sub(v_reflex.point, edge_a.point), n_k)
    tau = (d - v_reflex.t * dot_vel_nk) / denom

    if tau < v_reflex.t - EPS:
        return None
    tau = max(tau, v_reflex.t)

    # Position of reflex vertex at time tau
    dt = tau - v_reflex.t
    B = (v_reflex.point[0] + dt * vel[0],
         v_reflex.point[1] + dt * vel[1])

    # Compute moving positions of edge endpoints at time tau
    vel_a = _vertex_velocity(edge_a)
    vel_b = _vertex_velocity(edge_b)

    dt_a = tau - edge_a.t
    dt_b = tau - edge_b.t

    A_tau = (edge_a.point[0] + dt_a * vel_a[0],
             edge_a.point[1] + dt_a * vel_a[1])
    B_tau = (edge_b.point[0] + dt_b * vel_b[0],
             edge_b.point[1] + dt_b * vel_b[1])

    # Check B lies strictly inside the moving edge extent
    edge_vec = g.sub(B_tau, A_tau)
    edge_len = g.length(edge_vec)
    if edge_len < EPS:
        return None

    # Parameterize: u in [0, 1] means B lies on the segment A_tau..B_tau
    AB = g.sub(B, A_tau)
    u = g.dot(AB, edge_vec) / (edge_len * edge_len)

    # Must be strictly inside (not at the endpoints)
    margin = EPS * 10
    if u < margin or u > 1.0 - margin:
        return None

    # Must not be coincident with v_reflex (trivial/zero-advance split)
    if abs(tau - v_reflex.t) < EPS:
        return None

    return (tau, B)


def _push_split_events(heap, counter, v_reflex: "Vertex",
                       all_active_verts: list["Vertex"]) -> None:
    """Push all split-event candidates for a reflex vertex onto the heap."""
    if not v_reflex.is_reflex:
        return
    for ea in all_active_verts:
        if not ea.active:
            continue
        # Skip edges adjacent to v_reflex
        if ea is v_reflex or ea is v_reflex.prev:
            continue
        # Also skip ea.next if it equals v_reflex (other adjacent edge)
        if ea.next is v_reflex:
            continue
        result = _split_event_candidate(v_reflex, ea)
        if result is None:
            continue
        tau, B = result
        # (_SPLIT_EVENT, t, seq, B, v_reflex, edge_a)
        heapq.heappush(heap, (tau, _SPLIT_EVENT, next(counter), B, v_reflex, ea))


# ---------------------------------------------------------------------------
# Active-ring traversal
# ---------------------------------------------------------------------------

def _walk_active_ring(start: "Vertex") -> list["Vertex"]:
    """Return all active vertices in ring order starting from *start*.

    Uses the rewired .next/.prev pointers (inactive nodes are bypassed when
    wiring new vertices into the ring).
    """
    result = [start]
    v = start.next
    while v is not start:
        result.append(v)
        v = v.next
        if len(result) > 10_000:  # safety valve
            break
    return result


def _ring_size(start: "Vertex") -> int:
    """Count active vertices in a ring (with safety cap)."""
    count = 1
    v = start.next
    while v is not start:
        count += 1
        v = v.next
        if count > 10_000:
            return count
    return count


# ---------------------------------------------------------------------------
# skeletonize
# ---------------------------------------------------------------------------

def skeletonize(polygon_ccw: list[tuple]) -> SkeletonGraph:
    """Compute the straight skeleton of a simple (possibly non-convex) CCW polygon.

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
    max_iter = 20 * n * n  # generous cap for split events

    _counter = itertools.count()

    # --- Build initial event heap ---
    heap: list = []

    initial_verts = lav.vertices()
    for v in initial_verts:
        _push_edge_event(heap, _counter, v, v.next)

    # Push split events for all initial reflex vertices
    for v in initial_verts:
        if v.is_reflex:
            _push_split_events(heap, _counter, v, initial_verts)

    iterations = 0

    while heap:
        iterations += 1
        if iterations > max_iter:
            raise RoofGenerationError(
                f"Straight-skeleton simulation exceeded {max_iter} iterations."
            )

        event = heapq.heappop(heap)
        t = event[0]
        etype = event[1]

        # ----------------------------------------------------------------
        # Edge event: (t, _EDGE_EVENT, seq, I, V, Vnext)
        # ----------------------------------------------------------------
        if etype == _EDGE_EVENT:
            _t, _etype, _seq, I, V, Vnext = event

            # Skip stale events
            if not V.active or not Vnext.active:
                continue
            if V.next is not Vnext:
                continue

            ring_size = _ring_size(V)

            if ring_size == 3:
                # Triangle tail: three vertices meet at one final node.
                third = V.prev

                third_at_I = (
                    abs(third.point[0] - I[0]) < 1e-6 and
                    abs(third.point[1] - I[1]) < 1e-6
                )

                if not third_at_I:
                    # Genuine triangle collapse — new node; add all three arcs.
                    sg._add_node(I, t)
                    sg._add_arc(V.point, I)
                    sg._add_arc(Vnext.point, I)
                    sg._add_arc(third.point, I)
                else:
                    # Simultaneous collapse: the third vertex is already a node
                    # at I (created by the prior normal-merge event).  Just add
                    # the final two arcs.
                    sg._add_arc(V.point, I)
                    sg._add_arc(Vnext.point, I)

                V.active = False
                Vnext.active = False
                third.active = False
                continue

            if ring_size == 2:
                # Two-vertex ring: both collapse to the same point
                sg._add_node(I, t)
                sg._add_arc(V.point, I)
                sg._add_arc(Vnext.point, I)
                V.active = False
                Vnext.active = False
                continue

            # --- Normal edge-event: merge V and Vnext into new vertex M ---
            sg._add_node(I, t)
            sg._add_arc(V.point, I)
            sg._add_arc(Vnext.point, I)

            prev_v = V.prev
            next_vn = Vnext.next

            M = Vertex(I)
            M.t = t
            M.edge_left = V.edge_left
            M.edge_right = Vnext.edge_right

            M.prev = prev_v
            M.next = next_vn
            prev_v.next = M
            next_vn.prev = M

            # Compute bisector for M
            c = g.cross(M.edge_left, M.edge_right)
            M.is_reflex = c < -EPS
            M.bisector = _compute_bisector(
                prev_v.point, M.point, next_vn.point,
                M.edge_left, M.edge_right,
                M.is_reflex
            )

            V.active = False
            Vnext.active = False

            if lav.head is V or lav.head is Vnext:
                lav.head = M

            # Push new edge events involving M
            _push_edge_event(heap, _counter, M.prev, M)
            _push_edge_event(heap, _counter, M, M.next)

            # If M is reflex, push split events
            if M.is_reflex:
                ring_verts = _walk_active_ring(M)
                _push_split_events(heap, _counter, M, ring_verts)

        # ----------------------------------------------------------------
        # Split event: (t, _SPLIT_EVENT, seq, B, V_reflex, edge_a)
        # ----------------------------------------------------------------
        elif etype == _SPLIT_EVENT:
            _t, _etype, _seq, B, V_reflex, edge_a = event

            # Stale check
            if not V_reflex.active:
                continue
            if not edge_a.active:
                continue
            edge_b = edge_a.next
            if not edge_b.active:
                continue
            # Verify edge_a.next is still edge_b (ring not rewired around it)
            if edge_a.next is not edge_b:
                continue
            # Verify V_reflex is not now adjacent to edge_a (ring may have changed)
            if V_reflex.next is edge_a or V_reflex.prev is edge_a:
                continue
            if V_reflex.next is edge_b or V_reflex.prev is edge_b:
                continue

            # Record the valley arc from the reflex vertex to the split point
            sg._add_arc(V_reflex.point, B)
            sg._add_node(B, t)

            # --- Split the ring into two rings ---
            #
            # Before:  ... → prev_r → V_reflex → next_r → ... → edge_a → edge_b → ...
            # (it's one ring, traversed CCW)
            #
            # We create two new vertices B1 and B2 at position B, one for each ring.
            #
            # Ring 1: ... → prev_r → B1 → edge_b → ... → prev_r (the right sub-ring)
            # Ring 2: ... → edge_a → B2 → next_r → ... → edge_a (the left sub-ring)
            #
            # B1 inherits V_reflex's left edge and edge_a's right edge
            # B2 inherits edge_a's edge direction and V_reflex's right edge

            prev_r = V_reflex.prev
            next_r = V_reflex.next

            # B1: sits between prev_r and edge_b
            B1 = Vertex(B)
            B1.t = t
            B1.edge_left = V_reflex.edge_left   # edge coming from prev_r direction
            B1.edge_right = edge_a.edge_right   # edge going toward edge_b

            # B2: sits between edge_a and next_r
            B2 = Vertex(B)
            B2.t = t
            B2.edge_left = edge_a.edge_right    # symmetric to B1's right
            B2.edge_right = V_reflex.edge_right  # edge going toward next_r

            # Wire ring 1: prev_r → B1 → edge_b → ... (loops back to prev_r)
            B1.prev = prev_r
            B1.next = edge_b
            prev_r.next = B1
            edge_b.prev = B1

            # Wire ring 2: edge_a → B2 → next_r → ... (loops back to edge_a)
            B2.prev = edge_a
            B2.next = next_r
            edge_a.next = B2
            next_r.prev = B2

            # Compute bisectors for B1 and B2
            c1 = g.cross(B1.edge_left, B1.edge_right)
            B1.is_reflex = c1 < -EPS
            B1.bisector = _compute_bisector(
                prev_r.point, B1.point, edge_b.point,
                B1.edge_left, B1.edge_right,
                B1.is_reflex
            )

            c2 = g.cross(B2.edge_left, B2.edge_right)
            B2.is_reflex = c2 < -EPS
            B2.bisector = _compute_bisector(
                edge_a.point, B2.point, next_r.point,
                B2.edge_left, B2.edge_right,
                B2.is_reflex
            )

            # Deactivate the reflex vertex
            V_reflex.active = False

            # Update lav head if needed
            if lav.head is V_reflex:
                lav.head = B1

            # Push edge events for the new vertices and their neighbours
            _push_edge_event(heap, _counter, B1.prev, B1)
            _push_edge_event(heap, _counter, B1, B1.next)
            _push_edge_event(heap, _counter, B2.prev, B2)
            _push_edge_event(heap, _counter, B2, B2.next)

            # Gather both new rings and push split events for reflex vertices
            ring1_verts = _walk_active_ring(B1)
            ring2_verts = _walk_active_ring(B2)

            for rv in ring1_verts:
                if rv.is_reflex:
                    _push_split_events(heap, _counter, rv, ring1_verts)

            for rv in ring2_verts:
                if rv.is_reflex:
                    _push_split_events(heap, _counter, rv, ring2_verts)

    return sg
