"""Per-plane roof face extraction from the straight skeleton.

Build exactly one planar FaceResult per polygon edge using a DCEL (doubly-connected
edge list) half-edge traversal of the planar graph formed by the polygon edges and
the skeleton arcs.
"""
from __future__ import annotations

import math
from math import atan2, cos, radians, tan

from tectora.core.footprint import Footprint
from tectora.core.gable import folded_skeleton
from tectora.core.result import FaceResult

# Precision for rounding coordinates to a canonical key
_ROUND = 5
# Minimum 2-D face area to keep (filters degenerate cycles)
_EPS_AREA = 1e-8


def _key(pt: tuple) -> tuple:
    return (round(float(pt[0]), _ROUND), round(float(pt[1]), _ROUND))


def _signed_shoelace(pts: list[tuple]) -> float:
    """Signed shoelace area — positive for CCW."""
    n = len(pts)
    return 0.5 * sum(
        pts[i][0] * pts[(i + 1) % n][1] - pts[(i + 1) % n][0] * pts[i][1]
        for i in range(n)
    )


def build_faces(footprint: Footprint, pitch_deg: float) -> list[FaceResult]:
    """Extract one planar FaceResult per polygon edge from the straight skeleton.

    Parameters
    ----------
    footprint : Footprint
        CCW polygon.
    pitch_deg : float
        Roof pitch angle in degrees.

    Returns
    -------
    list[FaceResult]  — exactly one per polygon edge, all areas positive, 3-D
                        vertices lifted by z = t * tan(pitch_deg).
    """
    poly = list(footprint.vertices)
    # Compute the hipped skeleton then fold every `gable` edge (Stage 1). The
    # vertical gable-wall triangle projects to zero 2-D area and is therefore
    # dropped by the positive-area filter below; the two adjacent roof faces
    # extend to M. Shares the fold with roof_from_skeleton.build_roof.
    folded = folded_skeleton(footprint)
    slope = tan(radians(pitch_deg))
    cos_pitch = cos(radians(pitch_deg))

    # ------------------------------------------------------------------
    # 1. Collect all points and build a t-lookup (offset height).
    # ------------------------------------------------------------------
    # node_t: rounded key -> t. folded.node_t is keyed at precision _ROUND (6);
    # rebuild at this module's _key precision (5) so lookups line up.
    node_t: dict[tuple, float] = {}

    # Polygon vertices are at t=0
    for p in poly:
        node_t[_key(p)] = 0.0

    # Folded skeleton nodes carry their offset height (gable apexes already
    # moved to M).
    for x, y, t in folded.nodes:
        k = _key((x, y))
        if k not in node_t:
            node_t[k] = t
        else:
            # Keep maximum in case of duplicates from perturbation
            node_t[k] = max(node_t[k], t)

    def get_t(pt: tuple) -> float:
        return node_t.get(_key(pt), 0.0)

    # ------------------------------------------------------------------
    # 2. Build the planar graph: collect all undirected edges.
    # ------------------------------------------------------------------
    # edges_set: frozenset of two keys -> canonical undirected edge
    # adj: key -> list of neighbour keys (for sorting by angle)
    adj: dict[tuple, list[tuple]] = {}
    # coord_of_key: key -> (x, y) representative
    coord: dict[tuple, tuple] = {}

    def add_edge(a: tuple, b: tuple) -> None:
        ka, kb = _key(a), _key(b)
        if ka == kb:
            return  # skip zero-length
        # Register coordinates
        if ka not in coord:
            coord[ka] = (float(a[0]), float(a[1]))
        if kb not in coord:
            coord[kb] = (float(b[0]), float(b[1]))
        # Add to adjacency (duplicates handled in sort/dedup step)
        if ka not in adj:
            adj[ka] = []
        if kb not in adj:
            adj[kb] = []
        if kb not in adj[ka]:
            adj[ka].append(kb)
        if ka not in adj[kb]:
            adj[kb].append(ka)

    # Polygon edges. A gabled edge AB is NOT added: its two rake arcs A-M and
    # B-M (in folded.arcs) lie on the same line and subdivide AB at the apex
    # foot M, so adding AB as one full-span edge would overlap them and corrupt
    # the planar graph. The rakes serve as the face boundary on the gable side.
    n = len(poly)
    for i in range(n):
        if i in folded.gable_edges:
            continue
        add_edge(poly[i], poly[(i + 1) % n])

    # Folded skeleton arcs (gable apex endpoints already remapped to M; for a
    # gabled edge these include the rakes A-M / B-M that replace edge AB).
    for (ax, ay), (bx, by) in folded.arcs:
        add_edge((ax, ay), (bx, by))

    # ------------------------------------------------------------------
    # 3. Sort each vertex's neighbours by angle (CCW from +x axis).
    # ------------------------------------------------------------------
    for k in adj:
        cx, cy = coord[k]
        adj[k].sort(key=lambda nb: atan2(coord[nb][1] - cy, coord[nb][0] - cx))
        # Deduplicate (should already be unique but be safe)
        seen: set = set()
        deduped = []
        for nb in adj[k]:
            if nb not in seen:
                seen.add(nb)
                deduped.append(nb)
        adj[k] = deduped

    # ------------------------------------------------------------------
    # 4. Half-edge traversal (DCEL face extraction).
    #
    # For directed half-edge u→v, next(u→v) is the half-edge out of v
    # that is immediately CLOCKWISE from the reverse direction v→u.
    #
    # "Immediately clockwise from v→u at vertex v" means:
    # In the CCW-sorted neighbour list at v, find the index of u,
    # then take the entry at index-1 (mod len) — one step CW.
    # ------------------------------------------------------------------
    def next_half_edge(u_key: tuple, v_key: tuple) -> tuple[tuple, tuple]:
        """Return the half-edge (v→w) that follows (u→v) in the face traversal."""
        neighbours = adj[v_key]
        idx_u = neighbours.index(u_key)
        # One step clockwise = index - 1 in CCW-sorted list
        w_key = neighbours[(idx_u - 1) % len(neighbours)]
        return (v_key, w_key)

    # Enumerate all directed half-edges (both directions of each undirected edge)
    all_directed: set[tuple[tuple, tuple]] = set()
    for u_key, nbrs in adj.items():
        for v_key in nbrs:
            all_directed.add((u_key, v_key))

    # Traverse faces
    used: set[tuple[tuple, tuple]] = set()
    raw_faces: list[list[tuple]] = []  # each face = list of (x, y) keys in order

    for start_he in all_directed:
        if start_he in used:
            continue
        # Walk the face cycle
        cycle_keys: list[tuple] = []
        he = start_he
        for _ in range(len(all_directed) + 1):
            if he in used:
                break
            used.add(he)
            cycle_keys.append(he[0])
            he = next_half_edge(he[0], he[1])
            if he == start_he:
                break
        if len(cycle_keys) >= 3:
            raw_faces.append(cycle_keys)

    # ------------------------------------------------------------------
    # 5. Keep only CCW (positive signed area) faces; discard the outer face.
    # ------------------------------------------------------------------
    faces_2d: list[list[tuple]] = []
    for cycle in raw_faces:
        pts_2d = [coord[k] for k in cycle]
        area = _signed_shoelace(pts_2d)
        if area > _EPS_AREA:
            faces_2d.append(cycle)

    # ------------------------------------------------------------------
    # 6. Lift to 3-D and build FaceResult objects.
    # ------------------------------------------------------------------
    results: list[FaceResult] = []
    for i, cycle in enumerate(faces_2d):
        pts_2d = [coord[k] for k in cycle]
        area_2d = abs(_signed_shoelace(pts_2d))
        area_3d = area_2d / cos_pitch
        vertices_3d = [
            (coord[k][0], coord[k][1], get_t((coord[k][0], coord[k][1])) * slope)
            for k in cycle
        ]
        results.append(FaceResult(
            name=f"rovina {i + 1}",
            area_m2=area_3d,
            slope_deg=pitch_deg,
            vertices=vertices_3d,
        ))

    return results
