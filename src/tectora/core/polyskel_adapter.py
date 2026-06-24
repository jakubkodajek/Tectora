"""Adapter around the vendored polyskel straight-skeleton implementation.

Converts polyskel's ``Subtree(source, height, sinks)`` output into the
``SkeletonGraph`` (nodes ``(x, y, t)`` + arcs ``((x1,y1),(x2,y2))``) consumed by
``roof_from_skeleton.build_roof``.

Orientation: polyskel treats the y-axis as pointing downwards, so a standard
y-up CCW footprint must be reversed before handing it over (verified against the
hand-computed L-shape golden).

This module is our own (MIT) code; ``tectora.core._vendor.polyskel`` is
third-party LGPL — see its header and LICENSE.
"""
from __future__ import annotations

from tectora.core._vendor import polyskel as _polyskel
from tectora.core.straight_skeleton import SkeletonGraph


def polyskel_skeleton(polygon_ccw: list[tuple]) -> SkeletonGraph:
    """Compute the straight skeleton of a simple y-up CCW polygon via polyskel.

    Returns a ``SkeletonGraph`` whose nodes are the skeleton sources (with their
    offset height ``t``) and whose arcs connect each source to its sinks
    (original polygon vertices or other sources). Self-loops are dropped.
    """
    contour = [(float(x), float(y)) for x, y in reversed(list(polygon_ccw))]
    subtrees = _polyskel.skeletonize(contour, [])

    sg = SkeletonGraph()
    for st in subtrees:
        sx, sy = float(st.source.x), float(st.source.y)
        sg.nodes.append((sx, sy, float(st.height)))
        for sink in st.sinks:
            kx, ky = float(sink.x), float(sink.y)
            if abs(kx - sx) < 1e-9 and abs(ky - sy) < 1e-9:
                continue  # skip degenerate self-sink
            sg._add_arc((sx, sy), (kx, ky))
    return sg
