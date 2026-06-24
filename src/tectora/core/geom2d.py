"""Pure 2-D geometry helpers operating on (x, y) float tuples.

All functions are stateless and dependency-free.
"""

import math

EPS = 1e-9


# ---------------------------------------------------------------------------
# Basic vector operations
# ---------------------------------------------------------------------------

def sub(a: tuple, b: tuple) -> tuple:
    """Return a - b."""
    return (a[0] - b[0], a[1] - b[1])


def add(a: tuple, b: tuple) -> tuple:
    """Return a + b."""
    return (a[0] + b[0], a[1] + b[1])


def scale(a: tuple, s: float) -> tuple:
    """Return a * s."""
    return (a[0] * s, a[1] * s)


def dot(a: tuple, b: tuple) -> float:
    """Return the dot product of 2-D vectors a and b."""
    return a[0] * b[0] + a[1] * b[1]


def cross(a: tuple, b: tuple) -> float:
    """Return the 2-D scalar cross product ax*by - ay*bx."""
    return a[0] * b[1] - a[1] * b[0]


def length(a: tuple) -> float:
    """Return the Euclidean length of vector a."""
    return math.sqrt(a[0] * a[0] + a[1] * a[1])


def normalize(a: tuple) -> tuple:
    """Return the unit vector in the direction of a.

    Raises ValueError if a is the zero vector.
    """
    n = length(a)
    if n < EPS:
        raise ValueError(f"Cannot normalize zero-length vector: {a}")
    return (a[0] / n, a[1] / n)


# ---------------------------------------------------------------------------
# Geometric predicates
# ---------------------------------------------------------------------------

def orientation(a: tuple, b: tuple, c: tuple) -> float:
    """Return the signed area (cross product) of the triangle a-b-c.

    > 0 : left turn / CCW
    < 0 : right turn / CW
    = 0 : collinear  (within EPS, exactly 0.0 is returned)
    """
    val = cross(sub(b, a), sub(c, a))
    if abs(val) < EPS:
        return 0.0
    return val


# ---------------------------------------------------------------------------
# Line / segment intersections
# ---------------------------------------------------------------------------

def line_intersection(p1: tuple, d1: tuple, p2: tuple, d2: tuple):
    """Intersect the infinite line through p1 with direction d1
    and the infinite line through p2 with direction d2.

    Returns the intersection point as (x, y), or None if the lines are
    parallel (cross(d1, d2) < EPS).
    """
    denom = cross(d1, d2)
    if abs(denom) < EPS:
        return None  # parallel or coincident

    # p1 + t * d1 = p2 + s * d2
    # t = cross(p2 - p1, d2) / cross(d1, d2)
    diff = sub(p2, p1)
    t = cross(diff, d2) / denom
    return (p1[0] + t * d1[0], p1[1] + t * d1[1])


def ray_segment_intersection(origin: tuple, direction: tuple,
                              s1: tuple, s2: tuple):
    """Intersect a ray (origin + t*direction, t >= 0) with segment s1..s2.

    Returns the intersection point if the ray hits the segment in front of
    the origin (t > -EPS) and within the segment (0 <= u <= 1), else None.
    """
    seg_dir = sub(s2, s1)
    denom = cross(direction, seg_dir)
    if abs(denom) < EPS:
        return None  # parallel

    diff = sub(s1, origin)
    t = cross(diff, seg_dir) / denom
    u = cross(diff, direction) / denom

    if t < -EPS:
        return None  # intersection is behind the ray origin
    if u < -EPS or u > 1.0 + EPS:
        return None  # outside the segment

    return (origin[0] + t * direction[0], origin[1] + t * direction[1])


# ---------------------------------------------------------------------------
# Distance
# ---------------------------------------------------------------------------

def dist_point_to_line(p: tuple, a: tuple, b: tuple) -> float:
    """Return the perpendicular distance from point p to the infinite
    line defined by points a and b.
    """
    ab = sub(b, a)
    ab_len = length(ab)
    if ab_len < EPS:
        return length(sub(p, a))
    # |cross(ab, ap)| / |ab|
    ap = sub(p, a)
    return abs(cross(ab, ap)) / ab_len
