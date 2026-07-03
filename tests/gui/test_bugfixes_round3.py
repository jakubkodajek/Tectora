"""Round-3 bug fixes.

1. Excel export uses the same calculation as the screen (half-hip + back pitch
   were silently dropped before).
2. "Uložit do souboru…" (.tectora) saves ALL buildings, not just the active one.
3. Vymazat (clear) re-syncs the right panel to the fresh building's defaults.
4. Dragging a vertex cannot make the closed polygon self-intersect (the roof
   used to silently vanish).
5. The home-card thumbnail maps every building of a design into one shared box.
"""
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
    w.open_editor(Project(project_name="Test", global_pitch_deg=35.0))
    return app, w, w._stack.widget(1)


RECT = [(0, 0), (10, 0), (10, 6), (0, 6)]


def _draw_rect(c, pts=RECT):
    for (x, y) in pts:
        c.add_point(x, y)
    c.close_polygon()


# ---------------------------------------------------------------------------
# 1. Excel export matches the on-screen calculation (half-hip / back pitch)
# ---------------------------------------------------------------------------

def _gable_indices(c):
    """Indices of the two SHORT edges of the drawn rectangle (the cap ends)."""
    n = len(c._vertices)
    lens = []
    for i in range(n):
        (ax, ay) = c._vertices[i]
        (bx, by) = c._vertices[(i + 1) % n]
        lens.append(((bx - ax) ** 2 + (by - ay) ** 2, i))
    lens.sort()
    return [i for (_l, i) in lens[:2]]


def test_export_includes_half_hip(tmp_path):
    from openpyxl import load_workbook
    from tectora.core.roof_generator import generate_roof

    app, w, e = _editor(tmp_path)
    c = e.canvas
    _draw_rect(c)
    for i in _gable_indices(c):
        c.toggle_edge_type(i)          # both cap ends → GABLE spec
    e._half_hip_spin.setValue(1.0)     # clip depth → HALF_HIP

    expected = generate_roof(c.footprint, 35.0, overhang=0.0,
                             half_hip_m=1.0).total_area_m2

    path = str(tmp_path / "vykaz.xlsx")
    e._export_to(path)

    ws = load_workbook(path)["Výkaz"]
    area = None
    for row in ws.iter_rows(min_col=1, max_col=2):
        if row[0].value == "Celková plocha střechy":
            area = float(row[1].value)
            break
    assert area is not None
    assert abs(area - round(expected, 2)) < 0.01


# ---------------------------------------------------------------------------
# 2. .tectora file save keeps every building
# ---------------------------------------------------------------------------

def test_save_to_file_keeps_all_buildings(tmp_path):
    from tectora.io.project_file import load_project_file

    app, w, e = _editor(tmp_path)
    c = e.canvas
    _draw_rect(c)
    e._pitch_spin.setValue(40)
    e._on_new_building()
    _draw_rect(c, [(20, 0), (26, 0), (26, 4), (20, 4)])
    e._pitch_spin.setValue(25)
    e._base_height_spin.setValue(3.0)

    path = str(tmp_path / "design.tectora")
    e._save_to_file(path)

    project, _note, _loc = load_project_file(path)
    drawn = [b for b in project.buildings if b.footprint is not None]
    assert len(drawn) == 2
    assert drawn[0].pitch_deg == 40.0
    assert drawn[1].pitch_deg == 25.0
    assert drawn[1].base_height_m == 3.0


def test_open_file_restores_all_buildings(tmp_path):
    app, w, e = _editor(tmp_path)
    c = e.canvas
    _draw_rect(c)
    e._on_new_building()
    _draw_rect(c, [(20, 0), (26, 0), (26, 4), (20, 4)])

    path = str(tmp_path / "design.tectora")
    e._save_to_file(path)

    app2, w2, e2 = _editor(tmp_path)
    e2._open_from_file(path)
    app.processEvents()
    drawn = [i for i in range(e2.canvas.building_count())
             if e2.canvas.building_footprint(i) is not None]
    assert len(drawn) == 2


# ---------------------------------------------------------------------------
# 3. Clear re-syncs the panel
# ---------------------------------------------------------------------------

def test_clear_resyncs_panel_to_defaults(tmp_path):
    app, w, e = _editor(tmp_path)
    c = e.canvas
    _draw_rect(c)
    e._pitch_spin.setValue(42)
    assert c.active_pitch() == 42
    c.clear()
    # The fresh building has the default pitch — the panel must show it too,
    # otherwise the next calculation silently uses a different value.
    assert c.active_pitch() == 35
    assert e.pitch == 35
    assert e._pitch_spin.value() == 35


# ---------------------------------------------------------------------------
# 4. Vertex drag cannot self-intersect a closed polygon
# ---------------------------------------------------------------------------

def test_move_vertex_rejects_self_intersection(tmp_path):
    app, w, e = _editor(tmp_path)
    c = e.canvas
    _draw_rect(c)
    before = list(c._vertices)
    # Moving corner (0,0) to (5,8) would cross the top edge → bowtie.
    c.move_vertex(0, 5, 8)
    assert c._vertices == before          # rejected, polygon unchanged
    assert c.footprint is not None
    # A legal move still works.
    c.move_vertex(0, -1, -1)
    assert c._vertices[0] == (-1.0, -1.0)
    assert c.footprint is not None


# ---------------------------------------------------------------------------
# 5. Multi-building thumbnail shares one bounding box
# ---------------------------------------------------------------------------

def test_thumbnail_maps_all_buildings_into_shared_box():
    from tectora.core.footprint import Footprint
    from tectora.gui.home.project_card import footprints_to_mini_points

    fp1 = Footprint.from_lists([(0, 0), (4, 0), (4, 4), (0, 4)], ["eave"] * 4)
    fp2 = Footprint.from_lists([(6, 0), (10, 0), (10, 4), (6, 4)], ["eave"] * 4)
    mapped = footprints_to_mini_points([fp1, fp2], 200, 120, pad=10)
    assert len(mapped) == 2
    all_pts = [p for pts in mapped for p in pts]
    assert all(10 - 1e-6 <= x <= 190 + 1e-6 for (x, _y) in all_pts)
    assert all(10 - 1e-6 <= y <= 110 + 1e-6 for (_x, y) in all_pts)
    # Relative placement preserved: fp2 lies strictly right of fp1.
    assert min(x for (x, _y) in mapped[1]) > max(x for (x, _y) in mapped[0]) - 1e-6
