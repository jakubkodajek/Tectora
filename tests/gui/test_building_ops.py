"""Building operations: rename (✎), duplicate (⧉), rotate 90° (⟳)."""
import os
os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")


def _editor(tmp_path):
    from PySide6.QtWidgets import QApplication
    from tectora.application.project import Project
    from tectora.gui.app_window import MainWindow
    from tectora.io.sqlite_library import SqliteProjectLibrary
    app = QApplication.instance() or QApplication([])
    lib = SqliteProjectLibrary(str(tmp_path / "lib.db"))
    w = MainWindow(library=lib)
    w.open_editor(Project(global_pitch_deg=35.0))
    return app, w, w._stack.widget(1)


RECT = [(0, 0), (10, 0), (10, 6), (0, 6)]


def _draw(c, pts=RECT):
    for (x, y) in pts:
        c.add_point(x, y)
    c.close_polygon()


# ---------------------------------------------------------------------------
# Rename
# ---------------------------------------------------------------------------

def test_rename_updates_list_and_is_undoable(tmp_path):
    app, w, e = _editor(tmp_path)
    c = e.canvas
    _draw(c)
    e._ask_building_name = lambda current: "Stodola"
    e._on_rename_building(0)
    assert c._buildings[0].name == "Stodola"
    assert e._building_rows[0][0].text() == "Stodola"
    assert c.undo()
    assert c._buildings[0].name == ""


def test_rename_cancel_changes_nothing(tmp_path):
    app, w, e = _editor(tmp_path)
    c = e.canvas
    _draw(c)
    undo_before = c.can_undo()
    e._ask_building_name = lambda current: None       # user pressed Cancel
    e._on_rename_building(0)
    assert c._buildings[0].name == ""
    # No undo snapshot was pushed for a no-op.
    assert c.can_undo() == undo_before


# ---------------------------------------------------------------------------
# Duplicate
# ---------------------------------------------------------------------------

def test_duplicate_copies_shape_and_params_beside_original(tmp_path):
    app, w, e = _editor(tmp_path)
    c = e.canvas
    _draw(c)
    e._pitch_spin.setValue(42)
    e._base_height_spin.setValue(3.0)
    c.set_building_name(0, "Dům")

    idx = c.duplicate_building(0)
    assert idx == 1 and c.active_index() == 1
    copy = c._buildings[1]
    assert copy.pitch == 42
    assert copy.base_height == 3.0
    assert copy.name == "Dům (kopie)"
    assert copy.closed
    # Placed right of the original: bbox width 10 + 1 m gap.
    assert copy.vertices[0] == (11.0, 0.0)
    # Shape preserved.
    assert len(copy.vertices) == 4


def test_duplicate_does_not_overlap_original(tmp_path):
    from tectora.core.geom2d import polygons_overlap
    app, w, e = _editor(tmp_path)
    c = e.canvas
    _draw(c)
    c.duplicate_building(0)
    assert not polygons_overlap(c._buildings[0].vertices, c._buildings[1].vertices)


def test_duplicate_undo_removes_copy(tmp_path):
    app, w, e = _editor(tmp_path)
    c = e.canvas
    _draw(c)
    c.duplicate_building(0)
    assert c.building_count() == 2
    assert c.undo()
    assert c.building_count() == 1


def test_duplicate_button_in_list_row(tmp_path):
    app, w, e = _editor(tmp_path)
    _draw(e.canvas)
    e._building_rows[0][2].click()        # ⧉
    assert e.canvas.building_count() == 2


# ---------------------------------------------------------------------------
# Rotate
# ---------------------------------------------------------------------------

def test_rotate_90_swaps_rectangle_dimensions(tmp_path):
    app, w, e = _editor(tmp_path)
    c = e.canvas
    _draw(c)                              # 10 × 6
    c.rotate_building(0)
    xs = [v[0] for v in c._vertices]
    ys = [v[1] for v in c._vertices]
    assert abs((max(xs) - min(xs)) - 6.0) < 1e-9
    assert abs((max(ys) - min(ys)) - 10.0) < 1e-9
    # Snapped back onto the 0.5 m grid.
    assert all(abs(x * 2 - round(x * 2)) < 1e-9 for x in xs + ys)


def test_rotate_four_times_restores_shape(tmp_path):
    app, w, e = _editor(tmp_path)
    c = e.canvas
    _draw(c, [(0, 0), (6, 0), (6, 3), (3, 3), (3, 6), (0, 6)])   # L-shape
    original = list(c._vertices)
    for _ in range(4):
        c.rotate_building(0)
    assert all(
        abs(a[0] - b[0]) < 1e-9 and abs(a[1] - b[1]) < 1e-9
        for a, b in zip(c._vertices, original)
    )


def test_rotate_keeps_gables_valid(tmp_path):
    app, w, e = _editor(tmp_path)
    c = e.canvas
    _draw(c)
    # Gables on both short (cap) edges of the rectangle.
    n = len(c._vertices)
    lens = sorted(
        (( (c._vertices[(i + 1) % n][0] - c._vertices[i][0]) ** 2
           + (c._vertices[(i + 1) % n][1] - c._vertices[i][1]) ** 2), i)
        for i in range(n)
    )
    for (_l, i) in lens[:2]:
        c.toggle_edge_type(i)
    c.rotate_building(0)
    assert c.revert_invalid_gables(0) == 0      # still valid caps after rotation
    e._on_calculate()
    assert e.last_result is not None


def test_rotate_is_undoable(tmp_path):
    app, w, e = _editor(tmp_path)
    c = e.canvas
    _draw(c)
    original = list(c._vertices)
    c.rotate_building(0)
    assert c._vertices != original
    assert c.undo()
    assert c._vertices == original


def test_rotate_toolbar_button_rotates_active(tmp_path):
    app, w, e = _editor(tmp_path)
    c = e.canvas
    _draw(c)
    e._rotate_btn.click()
    xs = [v[0] for v in c._vertices]
    assert abs((max(xs) - min(xs)) - 6.0) < 1e-9
