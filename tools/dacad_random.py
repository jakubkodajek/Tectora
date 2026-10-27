"""Generate random DACAD V1.13 projects for a mass parity test.

Each case is drawn from a seeded generator, so ``--seed`` and the case index
reproduce it exactly. The generator mixes the building blocks the hand-made
cases (tools/dacad_testcases.py) isolate one at a time: rectilinear outlines
(rectangle, L, T, U, notch, Z), hips and gables, mixed pitches, raised eaves,
overhangs, half-hips (stacked definitions), mansards, chimneys, roof windows
and gable or shed dormers.

    python tools/dacad_random.py OUT_DIR --count 1000 --seed 1

Writes OUT_DIR/R<seed>_<index>.DAD and OUT_DIR/index.json with each case's
parameters. Coordinates sit on a 5 cm grid and pitches are whole degrees.
"""
from __future__ import annotations

import argparse
import json
import math
import random
import sys
from dataclasses import asdict
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from dacad_testcases import Chimney, Def, Dormer, W, encode, rotated_rect  # noqa: E402

GRID = 0.05


def _q(rng: random.Random, lo: float, hi: float) -> float:
    """Uniform value on the 5 cm grid."""
    return round(round(rng.uniform(lo, hi) / GRID) * GRID, 2)


# Outlines as (length, direction) steps, counter-clockwise from (0, 0).
# Each returns the steps and the indices of "end" walls that may be gables.
def _rect(rng):
    a, b = _q(rng, 6, 16), _q(rng, 5, 11)
    return [(a, 0), (b, 90), (a, 180), (b, 270)], {1, 3}


def _l_shape(rng):
    a, b = _q(rng, 8, 16), _q(rng, 8, 14)
    wa, wb = _q(rng, 4, min(8, b - 2)), _q(rng, 4, min(8, a - 2))
    return [(a, 0), (wa, 90), (a - wb, 180), (b - wa, 90), (wb, 180), (b, 270)], {1, 4}


def _t_shape(rng):
    a, wa = _q(rng, 10, 18), _q(rng, 4, 8)
    wb, lb = _q(rng, 3.5, min(7, a - 4)), _q(rng, 3, 8)
    s = round(round((a - wb) / 2 / GRID) * GRID, 2)
    return [(a, 0), (wa, 90), (s, 180), (lb, 90), (wb, 180), (lb, 270),
            (round(a - s - wb, 2), 180), (wa, 270)], {1, 4, 7}


def _u_shape(rng):
    a, wa = _q(rng, 12, 18), _q(rng, 4, 7)
    wl, lb = _q(rng, 3.5, 6), _q(rng, 3, 7)
    if a - 2 * wl < 2:
        a = round(2 * wl + 2, 2)
    return [(a, 0), (wa + lb, 90), (wl, 180), (lb, 270), (round(a - 2 * wl, 2), 180),
            (lb, 90), (wl, 180), (wa + lb, 270)], {2, 6}


def _notch(rng):
    a, b = _q(rng, 8, 16), _q(rng, 6, 11)
    nx, nw, nd = _q(rng, 2, a / 2 - 1), _q(rng, 1, 3), _q(rng, 0.5, 2)
    d1, d2 = (90, 270) if rng.random() < 0.5 else (270, 90)
    return [(nx, 0), (nd, d1), (nw, 0), (nd, d2), (round(a - nx - nw, 2), 0),
            (b, 90), (a, 180), (b, 270)], {5, 7}


def _z_shape(rng):
    a, w = _q(rng, 6, 10), _q(rng, 4, 7)
    o, c = _q(rng, 2, a - 2), _q(rng, 3, 7)
    return [(a, 0), (w + c, 90), (o, 180), (c, 90), (a, 180), (w + c, 270),
            (o, 0), (c, 270)], {1, 4}


SHAPES = [(_rect, 35), (_l_shape, 25), (_t_shape, 12), (_u_shape, 10),
          (_notch, 12), (_z_shape, 6)]


def _closed(steps) -> bool:
    x = sum(l * math.cos(math.radians(d)) for l, d in steps)
    y = sum(l * math.sin(math.radians(d)) for l, d in steps)
    return math.hypot(x, y) < 1e-6 and all(l > 0.2 for l, _ in steps)


def _points(steps):
    x = y = 0.0
    pts = [(0.0, 0.0)]
    for l, d in steps:
        x += l * math.cos(math.radians(d))
        y += l * math.sin(math.radians(d))
        pts.append((round(x, 4), round(y, 4)))
    return pts[:-1]


def _inside(pts, x, y) -> bool:
    c = False
    for (x1, y1), (x2, y2) in zip(pts, pts[1:] + pts[:1]):
        if (y1 > y) != (y2 > y) and x < x1 + (y - y1) * (x2 - x1) / (y2 - y1):
            c = not c
    return c


def _dist_to_outline(pts, x, y) -> float:
    best = math.inf
    for (x1, y1), (x2, y2) in zip(pts, pts[1:] + pts[:1]):
        dx, dy = x2 - x1, y2 - y1
        t = max(0.0, min(1.0, ((x - x1) * dx + (y - y1) * dy) / (dx * dx + dy * dy)))
        best = min(best, math.hypot(x - x1 - t * dx, y - y1 - t * dy))
    return best


def _near_window(x: float, y: float, win, margin: float = 1.0) -> bool:
    """Whether a chimney drawn at (x, y) could overlap the roof window."""
    wx, wy, ww, wh = win
    return wx - margin < x < wx + ww + margin and wy - margin < y < wy + wh + margin


def generate(seed: int, index: int) -> tuple[bytes, dict]:
    rng = random.Random(seed * 1_000_003 + index)
    shape, steps, ends = None, None, None
    while steps is None or not _closed(steps):
        f = rng.choices([s for s, _ in SHAPES], [w for _, w in SHAPES])[0]
        shape = f.__name__.lstrip("_")
        steps, ends = f(rng)
    n = len(steps)
    pitch = rng.randint(20, 55)
    eave = _q(rng, 2.5, 4.0)
    ov = round(rng.choice([0, 0, 0.3, 0.5, 0.5, 0.6, 0.8]), 2)
    mixed = rng.random() < 0.35
    gable_p = 0.0 if rng.random() < 0.35 else rng.choice([0.5, 1.0])
    mansard = shape == "rect" and rng.random() < 0.08
    walls: list[W] = []
    for i, (l, d) in enumerate(steps):
        p = rng.randint(20, 60) if mixed and rng.random() < 0.4 else pitch
        e = eave
        if rng.random() < 0.12:
            e = round(eave + rng.choice([-1, 1]) * _q(rng, 0.2, 1.0), 2)
        o = ov if rng.random() < 0.8 else round(rng.choice([0, 0.3, 0.5, 0.8]), 2)
        if i in ends and rng.random() < gable_p:
            base = Def(90.0, o, e)
            stacked = []
            if rng.random() < 0.3:
                # Half-hip from part way up the gable.
                stacked = [Def(rng.randint(45, 70), 0.0, round(e + _q(rng, 1.0, 2.5), 2))]
            walls.append(W(l, d, base, stacked))
            continue
        base = Def(float(p), o, e, prio=1 if rng.random() < 0.05 else 0)
        stacked = [Def(rng.randint(25, 35), 0.0, round(e + _q(rng, 1.2, 2.0), 2))] if mansard else []
        walls.append(W(l, d, base, stacked))
    if all(w.base.pitch >= 90 for w in walls):
        walls[0].base = Def(float(pitch), ov, eave)
    pts = _points(steps)

    # Roof windows on the first wall's plane (the long south eave).
    w0 = walls[0]
    if w0.base.pitch < 90 and not w0.stacked and w0.length > 5 and rng.random() < 0.3:
        for _ in range(rng.choice([1, 1, 2])):
            ww, wh = _q(rng, 0.6, 1.4), _q(rng, 0.8, 1.6)
            x = _q(rng, 1.2, w0.length - ww - 1.2)
            y = _q(rng, 1.0, 2.0)
            if not any(abs(x - wx) < ww + 0.5 for wx, *_ in w0.windows):
                w0.windows.append((x, y, ww, wh))

    # Chimneys well inside the outline.
    chimneys: list[Chimney] = []
    for _ in range(rng.choice([0, 0, 0, 1, 1, 2])):
        for _try in range(20):
            x, y = _q(rng, 0.5, 18), _q(rng, 0.5, 14)
            a, b = _q(rng, 0.4, 1.0), _q(rng, 0.4, 1.0)
            if _inside(pts, x, y) and _dist_to_outline(pts, x, y) > 1.2 and \
                    all(math.hypot(x - c.corners[0][0], y - c.corners[0][1]) > 2 for c in chimneys) and \
                    not any(_near_window(x, y, w) for w in w0.windows):
                rot = rng.choice([0, 0, 0, 30, 45])
                corners = [(round(cx, 4), round(cy, 4)) for cx, cy in rotated_rect(x, y, a, b, rot)]
                chimneys.append(Chimney(round(eave + 12, 2), corners))
                break

    # One gable or shed dormer on the first wall.
    dormers: list[Dormer] = []
    if w0.base.pitch < 75 and not w0.stacked and not w0.windows and w0.length > 6 \
            and w0.base.eave_h == eave and rng.random() < 0.25:
        dw = _q(rng, 1.5, 3.0)
        dx = _q(rng, 1.5, w0.length - dw - 1.5)
        back = rng.choice([0.0, 0.5, 1.0])
        de = round(eave + _q(rng, 0.6, 1.5), 2)
        if rng.random() < 0.7:
            dp = float(rng.randint(35, 55))
            dormers.append(Dormer((dx, back), [W(0, 270, Def(dp, 0, de)), W(dw, 0, Def(90, 0, de)),
                                               W(0, 90, Def(dp, 0, de))]))
        else:
            dormers.append(Dormer((dx, back), [W(0, 270, Def(90, 0, de)),
                                               W(dw, 0, Def(float(rng.randint(10, 20)), 0, de)),
                                               W(0, 90, Def(90, 0, de))]))

    params = {"shape": shape, "walls": [asdict(w) for w in walls],
              "chimneys": [asdict(c) for c in chimneys],
              "dormers": [asdict(d) for d in dormers]}
    return encode(walls, chimneys, dormers), params


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("out", type=Path)
    ap.add_argument("--count", type=int, default=1000)
    ap.add_argument("--seed", type=int, default=1)
    ap.add_argument("--start", type=int, default=0)
    a = ap.parse_args(argv)
    a.out.mkdir(parents=True, exist_ok=True)
    index = {}
    for i in range(a.start, a.start + a.count):
        data, params = generate(a.seed, i)
        name = f"R{a.seed}_{i:04d}"
        (a.out / f"{name}.DAD").write_bytes(data)
        index[name] = params
    (a.out / "index.json").write_text(json.dumps(index, indent=1))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
