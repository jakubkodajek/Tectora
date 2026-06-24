"""Gable (štít) support for the straight-skeleton roof path — Stage 1.

A Stage-1 gable is a *rectangular terminal cap*: a footprint edge whose two
neighbour edges are perpendicular to it and lie on the same side, and whose
straight skeleton has a single apex node joined to both edge endpoints (one
triangular cap face).

This module provides:

* :func:`is_valid_gable_edge` — pure domain predicate (no UI), used by the core
  geometry, the GUI editor, and the CLI.
* :class:`GableNotSupported` — raised when ``build_roof`` is asked to gable an
  edge that fails validation.
* :func:`folded_skeleton` — the shared fold used by both ``roof_from_skeleton``
  and ``roof_faces`` so they agree on the modified node set / arcs / per-arc
  classification.

The fold for a valid cap edge ``(A, B)`` with apex ``P`` (offset height ``t_P``):
move ``P`` horizontally to ``M`` (the perpendicular foot of ``P`` on segment
``AB``; the midpoint for a symmetric cap), keeping height ``t_P``. Triangle
``A-B-M`` is then vertical (the gable wall, projects to zero 2-D area). The arcs
``A-P``, ``B-P`` become rake edges ``A-M``, ``B-M`` (kind ``"gable"``, NOT hips);
the ridge that ended at ``P`` now extends to ``M``.
"""
from __future__ import annotations

import math

from tectora.core import geom2d as g
from tectora.core.footprint import EdgeType, Footprint
from tectora.core.polyskel_adapter import polyskel_skeleton
from tectora.core.straight_skeleton import build_lav

# Coordinate rounding precision for node keys (matches roof_from_skeleton).
_ROUND = 6
# Tolerance on the normalized dot product for the perpendicularity test.
_PERP_TOL = 1e-6
# Tolerance for matching a skeleton arc endpoint to a polygon vertex.
_MATCH_TOL = 1e-4


class GableNotSupported(ValueError):
    """Raised when an edge marked ``gable`` is not a valid Stage-1 cap.

    Stage 1 only supports gables on a rectangular *terminal cap* edge (a wing
    end). Any other placement (a long/inner edge, or a reflex-adjacent edge)
    must be rejected rather than emitting a broken roof. Inherits from
    ``ValueError`` so the GUI/CLI report it gracefully.
    """


def _round2(pt: tuple) -> tuple:
    return (round(float(pt[0]), _ROUND), round(float(pt[1]), _ROUND))


def _convex_vertices(footprint: Footprint) -> set:
    """Return the set of rounded (x, y) keys for convex (non-reflex) corners."""
    reflex: set = set()
    for v in build_lav(list(footprint.vertices)).vertices():
        if v.is_reflex:
            reflex.add(_round2(v.point))
    return {_round2(p) for p in footprint.vertices} - reflex


def _is_reflex_vertex(footprint: Footprint, pt: tuple) -> bool:
    for v in build_lav(list(footprint.vertices)).vertices():
        if _round2(v.point) == _round2(pt):
            return bool(v.is_reflex)
    return False


def _find_cap_apex(graph, a: tuple, b: tuple):
    """Return the single skeleton apex node joined by arcs to BOTH *a* and *b*.

    Returns ``(px, py, t_p)`` for that node, or ``None`` if there is not exactly
    one such node (so the edge is not a single triangular cap).
    """
    ka, kb = _round2(a), _round2(b)

    # Neighbours of each polygon endpoint among the skeleton arcs.
    def neighbours(k: tuple) -> set:
        out: set = set()
        for (ax, ay), (bx, by) in graph.arcs:
            ea, eb = _round2((ax, ay)), _round2((bx, by))
            if ea == k:
                out.add(eb)
            elif eb == k:
                out.add(ea)
        return out

    common = neighbours(ka) & neighbours(kb)
    # Drop the endpoints themselves (an A-B polygon edge is not in graph.arcs,
    # but guard anyway) and keep only genuine interior apex nodes.
    common.discard(ka)
    common.discard(kb)
    if len(common) != 1:
        return None
    apex_key = next(iter(common))

    # Resolve the apex's offset height t from the node list.
    for x, y, t in graph.nodes:
        if _round2((x, y)) == apex_key:
            return (float(x), float(y), float(t))
    # Apex appears only as an arc endpoint (shouldn't happen for an interior
    # node, since interior nodes are always added to graph.nodes).
    return None


def is_valid_gable_edge(footprint: Footprint, i: int) -> bool:
    """``True`` iff edge *i* of *footprint* is a valid Stage-1 rectangular cap.

    Edge ``i`` is ``A = V[i] -> B = V[i+1]``. It is a valid Stage-1 gable iff:

    * ``A`` and ``B`` are both convex corners, AND
    * the previous edge ``V[i-1] -> A`` and the next edge ``B -> V[i+1]`` are
      both perpendicular to ``AB`` (within tolerance on the normalized dot
      product) and lie on the SAME side of line ``AB``, AND
    * the straight skeleton has exactly one apex node joined by arcs to BOTH
      ``A`` and ``B`` (one triangular cap face).

    Never raises — returns ``False`` for any non-cap edge.
    """
    try:
        n = footprint.n
        if n < 3:
            return False

        a = footprint.vertices[i]
        b = footprint.vertices[(i + 1) % n]
        prev = footprint.vertices[(i - 1) % n]
        nxt = footprint.vertices[(i + 2) % n]

        # Both endpoints convex.
        convex = _convex_vertices(footprint)
        if _round2(a) not in convex or _round2(b) not in convex:
            return False

        ab = g.sub(b, a)
        if g.length(ab) < g.EPS:
            return False
        ab_u = g.normalize(ab)

        # Previous edge (prev -> A) and next edge (B -> nxt) directions.
        e_prev = g.sub(a, prev)
        e_next = g.sub(nxt, b)
        if g.length(e_prev) < g.EPS or g.length(e_next) < g.EPS:
            return False
        e_prev_u = g.normalize(e_prev)
        e_next_u = g.normalize(e_next)

        # Perpendicular to AB (normalized dot ~ 0).
        if abs(g.dot(e_prev_u, ab_u)) > _PERP_TOL:
            return False
        if abs(g.dot(e_next_u, ab_u)) > _PERP_TOL:
            return False

        # Same side of line AB: the two neighbour vertices (prev and nxt) must
        # lie strictly on the same side of the infinite line AB, so that the
        # cap is a rectangular wing sticking out to one side (not a zig-zag).
        side_prev = g.cross(ab_u, g.sub(prev, a))
        side_next = g.cross(ab_u, g.sub(nxt, a))
        if side_prev * side_next <= _PERP_TOL:
            return False

        # Exactly one triangular cap apex joined to both A and B.
        graph = polyskel_skeleton(list(footprint.vertices))
        apex = _find_cap_apex(graph, a, b)
        return apex is not None
    except Exception:
        return False


def _perp_foot(p: tuple, a: tuple, b: tuple) -> tuple:
    """Perpendicular foot of point *p* onto the infinite line through a, b."""
    ab = g.sub(b, a)
    ab2 = g.dot(ab, ab)
    if ab2 < g.EPS:
        return (float(a[0]), float(a[1]))
    t = g.dot(g.sub(p, a), ab) / ab2
    return (a[0] + t * ab[0], a[1] + t * ab[1])


class FoldedSkeleton:
    """Result of folding the hipped skeleton over a footprint's gable edges.

    Attributes
    ----------
    nodes : list of (x, y, t)
        Skeleton interior nodes after the fold (gable apexes moved to M).
    arcs : list of ((x1, y1), (x2, y2))
        Skeleton arcs after the fold (apex endpoints remapped to M).
    rake_keys : set of frozenset({key_a, key_m})
        Undirected keys of arcs that are gable rakes (A-M, B-M). These are
        emitted as kind ``"gable"`` and excluded from hip totals.
    node_t : dict[key -> t]
        Offset height lookup including the moved M points (and polygon vertices
        at t = 0).
    gable_edges : set of int
        Indices of footprint edges that were folded as gables (for excluding
        from the eave perimeter).
    """

    __slots__ = ("nodes", "arcs", "rake_keys", "node_t", "gable_edges")

    def __init__(self, nodes, arcs, rake_keys, node_t, gable_edges):
        self.nodes = nodes
        self.arcs = arcs
        self.rake_keys = rake_keys
        self.node_t = node_t
        self.gable_edges = gable_edges


def folded_skeleton(footprint: Footprint) -> FoldedSkeleton:
    """Compute the hipped skeleton then fold every ``gable`` edge.

    Shared by ``roof_from_skeleton.build_roof`` and ``roof_faces.build_faces``
    so both agree on the folded geometry.

    Raises
    ------
    GableNotSupported
        If a footprint edge is typed ``gable`` but fails
        :func:`is_valid_gable_edge`.
    """
    poly = list(footprint.vertices)
    graph = polyskel_skeleton(poly)
    n = footprint.n

    poly_set = {_round2(p) for p in poly}

    # Base node_t (polygon vertices at t = 0; skeleton nodes carry t).
    node_t: dict = {}
    for x, y, t in graph.nodes:
        k = _round2((x, y))
        node_t[k] = max(node_t.get(k, t), t)
    for k in poly_set:
        node_t[k] = 0.0

    # Identify gable edges; validate each.
    gable_edges: set = set()
    # apex_key -> M (the moved position) for every folded apex.
    remap: dict = {}
    # Undirected keys of the rake arcs we create (A-M and B-M).
    rake_keys: set = set()

    for i, edge in enumerate(footprint.edges):
        if edge.type != EdgeType.GABLE:
            continue
        if not is_valid_gable_edge(footprint, i):
            raise GableNotSupported(
                f"Edge {i} is marked 'gable' but is not a Stage-1 rectangular "
                f"cap (a wing end with two perpendicular neighbour edges)."
            )
        a = footprint.vertices[i]
        b = footprint.vertices[(i + 1) % n]
        apex = _find_cap_apex(graph, a, b)
        # is_valid_gable_edge guarantees apex is not None.
        px, py, t_p = apex
        apex_key = _round2((px, py))
        m = _perp_foot((px, py), a, b)
        remap[apex_key] = m
        node_t[_round2(m)] = t_p
        gable_edges.add(i)
        rake_keys.add(frozenset((_round2(a), _round2(m))))
        rake_keys.add(frozenset((_round2(b), _round2(m))))

    def remap_pt(pt: tuple) -> tuple:
        k = _round2(pt)
        if k in remap:
            return remap[k]
        return (float(pt[0]), float(pt[1]))

    # Rebuild nodes with apex positions moved to M.
    new_nodes: list = []
    for x, y, t in graph.nodes:
        k = _round2((x, y))
        if k in remap:
            mx, my = remap[k]
            new_nodes.append((mx, my, t))
        else:
            new_nodes.append((float(x), float(y), t))

    # Rebuild arcs with apex endpoints remapped to M; drop zero-length arcs.
    new_arcs: list = []
    for (ax, ay), (bx, by) in graph.arcs:
        na = remap_pt((ax, ay))
        nb = remap_pt((bx, by))
        if abs(na[0] - nb[0]) < 1e-9 and abs(na[1] - nb[1]) < 1e-9:
            continue
        new_arcs.append((na, nb))

    return FoldedSkeleton(
        nodes=new_nodes,
        arcs=new_arcs,
        rake_keys=rake_keys,
        node_t=node_t,
        gable_edges=gable_edges,
    )


def gable_wall_triangles(
    footprint: Footprint, pitch_deg: float
) -> list[tuple[tuple, tuple, tuple]]:
    """Return the vertical gable-wall (štít) triangles for a footprint.

    One triangle per valid ``gable`` edge: ``(A, B, M)`` with
    ``A = (x, y, 0)`` and ``B = (x, y, 0)`` the eave corners and
    ``M = (mx, my, h)`` the ridge apex above the perpendicular foot on ``AB``,
    where ``h = t_P * tan(pitch)``. These are 3-D wall faces (not roof faces);
    the 3-D view renders them as the vertical triangular štít at each gabled
    wing end.

    Raises :class:`GableNotSupported` if a ``gable`` edge is not a valid cap.
    """
    slope = math.tan(math.radians(pitch_deg))
    poly = list(footprint.vertices)
    graph = polyskel_skeleton(poly)
    n = footprint.n
    walls: list[tuple[tuple, tuple, tuple]] = []
    for i, edge in enumerate(footprint.edges):
        if edge.type != EdgeType.GABLE:
            continue
        if not is_valid_gable_edge(footprint, i):
            raise GableNotSupported(
                f"Edge {i} is marked 'gable' but is not a Stage-1 cap."
            )
        a = footprint.vertices[i]
        b = footprint.vertices[(i + 1) % n]
        px, py, t_p = _find_cap_apex(graph, a, b)
        mx, my = _perp_foot((px, py), a, b)
        walls.append((
            (float(a[0]), float(a[1]), 0.0),
            (float(b[0]), float(b[1]), 0.0),
            (mx, my, t_p * slope),
        ))
    return walls


def is_valid_gable_edge_at(footprint: Footprint, p: tuple, q: tuple) -> bool:
    """``True`` iff the footprint edge with endpoints ``{p, q}`` is a valid cap.

    Matches the edge by its endpoint coordinates (order-independent) so callers
    that index a polygon in a *different* winding/order than the normalised
    ``Footprint`` (e.g. the 2-D canvas, which stores draw-order vertices while
    ``Footprint.from_lists`` re-winds to CCW and rotates the edge list) validate
    the edge they actually mean. Returns ``False`` if no edge matches.
    """
    kp, kq = _round2(p), _round2(q)
    n = footprint.n
    for i in range(n):
        a = _round2(footprint.vertices[i])
        b = _round2(footprint.vertices[(i + 1) % n])
        if {a, b} == {kp, kq}:
            return is_valid_gable_edge(footprint, i)
    return False
