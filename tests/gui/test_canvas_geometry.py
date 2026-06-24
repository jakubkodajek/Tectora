# -*- coding: utf-8 -*-
"""Pure unit tests for canvas_geometry helpers (no Qt required)."""
import pytest


# ---------------------------------------------------------------------------
# screen_to_model / model_to_screen round-trip
# ---------------------------------------------------------------------------

def test_round_trip_origin():
    from tectora.gui.editor.canvas_geometry import screen_to_model, model_to_screen
    origin_px = (400, 300)
    scale = 80  # 80 px/m
    mx, my = screen_to_model(400, 300, origin_px, scale)
    assert mx == pytest.approx(0.0)
    assert my == pytest.approx(0.0)
    px, py = model_to_screen(0.0, 0.0, origin_px, scale)
    assert px == pytest.approx(400.0)
    assert py == pytest.approx(300.0)


def test_round_trip_offset():
    from tectora.gui.editor.canvas_geometry import screen_to_model, model_to_screen
    origin_px = (100, 200)
    scale = 50
    # model point (3, -2)
    px, py = model_to_screen(3.0, -2.0, origin_px, scale)
    mx, my = screen_to_model(px, py, origin_px, scale)
    assert mx == pytest.approx(3.0)
    assert my == pytest.approx(-2.0)


def test_screen_to_model_basic():
    from tectora.gui.editor.canvas_geometry import screen_to_model
    # origin at (0,0), scale 100 px/m
    # screen (100, 50) -> model (1.0, -0.5) if Y is inverted (screen Y down = model Y up)
    origin_px = (0, 0)
    scale = 100
    mx, my = screen_to_model(100, 50, origin_px, scale)
    assert mx == pytest.approx(1.0)
    assert my == pytest.approx(-0.5)


def test_model_to_screen_basic():
    from tectora.gui.editor.canvas_geometry import model_to_screen
    origin_px = (0, 0)
    scale = 100
    px, py = model_to_screen(1.0, -0.5, origin_px, scale)
    assert px == pytest.approx(100.0)
    assert py == pytest.approx(50.0)


# ---------------------------------------------------------------------------
# snap_to_grid
# ---------------------------------------------------------------------------

def test_snap_to_grid_step_half():
    from tectora.gui.editor.canvas_geometry import snap_to_grid
    sx, sy = snap_to_grid(1.2, 2.49, 0.5)
    assert sx == pytest.approx(1.0)
    assert sy == pytest.approx(2.5)


def test_snap_to_grid_step_one():
    from tectora.gui.editor.canvas_geometry import snap_to_grid
    sx, sy = snap_to_grid(3.7, 0.3, 1.0)
    assert sx == pytest.approx(4.0)
    assert sy == pytest.approx(0.0)


def test_snap_to_grid_already_on_grid():
    from tectora.gui.editor.canvas_geometry import snap_to_grid
    sx, sy = snap_to_grid(2.0, 4.0, 0.5)
    assert sx == pytest.approx(2.0)
    assert sy == pytest.approx(4.0)


def test_snap_to_grid_negative():
    from tectora.gui.editor.canvas_geometry import snap_to_grid
    sx, sy = snap_to_grid(-1.3, -2.6, 0.5)
    assert sx == pytest.approx(-1.5)
    assert sy == pytest.approx(-2.5)


# ---------------------------------------------------------------------------
# hit_test_node
# ---------------------------------------------------------------------------

def test_hit_test_node_hit():
    from tectora.gui.editor.canvas_geometry import hit_test_node
    nodes = [(100.0, 100.0), (200.0, 150.0), (300.0, 200.0)]
    # Click exactly on node 1
    idx = hit_test_node(200.0, 150.0, nodes, radius=8.0)
    assert idx == 1


def test_hit_test_node_within_radius():
    from tectora.gui.editor.canvas_geometry import hit_test_node
    nodes = [(100.0, 100.0), (200.0, 150.0)]
    idx = hit_test_node(204.0, 153.0, nodes, radius=8.0)
    assert idx == 1  # distance = 5.0, within radius 8


def test_hit_test_node_miss():
    from tectora.gui.editor.canvas_geometry import hit_test_node
    nodes = [(100.0, 100.0), (200.0, 150.0)]
    idx = hit_test_node(215.0, 165.0, nodes, radius=8.0)
    assert idx is None


def test_hit_test_node_empty():
    from tectora.gui.editor.canvas_geometry import hit_test_node
    idx = hit_test_node(100.0, 100.0, [], radius=8.0)
    assert idx is None


def test_hit_test_node_prefers_first():
    from tectora.gui.editor.canvas_geometry import hit_test_node
    # Two nodes very close to each other; should return the first one hit
    nodes = [(100.0, 100.0), (103.0, 100.0)]
    idx = hit_test_node(101.0, 100.0, nodes, radius=8.0)
    assert idx == 0


# ---------------------------------------------------------------------------
# hit_test_edge
# ---------------------------------------------------------------------------

def test_hit_test_edge_on_segment():
    from tectora.gui.editor.canvas_geometry import hit_test_edge
    # Horizontal segment from (100,100) to (200,100)
    segs = [((100.0, 100.0), (200.0, 100.0))]
    idx = hit_test_edge(150.0, 100.0, segs, tol=6.0)
    assert idx == 0


def test_hit_test_edge_near_segment():
    from tectora.gui.editor.canvas_geometry import hit_test_edge
    segs = [((100.0, 100.0), (200.0, 100.0))]
    idx = hit_test_edge(150.0, 104.0, segs, tol=6.0)
    assert idx == 0  # 4 px away, within tol 6


def test_hit_test_edge_miss():
    from tectora.gui.editor.canvas_geometry import hit_test_edge
    segs = [((100.0, 100.0), (200.0, 100.0))]
    idx = hit_test_edge(150.0, 110.0, segs, tol=6.0)
    assert idx is None


def test_hit_test_edge_outside_projection():
    from tectora.gui.editor.canvas_geometry import hit_test_edge
    # Click is beyond the endpoint
    segs = [((100.0, 100.0), (200.0, 100.0))]
    idx = hit_test_edge(250.0, 100.0, segs, tol=6.0)
    assert idx is None


def test_hit_test_edge_multiple():
    from tectora.gui.editor.canvas_geometry import hit_test_edge
    segs = [
        ((0.0, 0.0), (100.0, 0.0)),
        ((100.0, 0.0), (100.0, 100.0)),
    ]
    idx = hit_test_edge(100.0, 50.0, segs, tol=6.0)
    assert idx == 1
