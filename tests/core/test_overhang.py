import math
import pytest
from tectora.core.footprint import Footprint
from tectora.core.roof_generator import generate_roof


def test_overhang_does_not_change_roof():
    # Tectora models the ROOF: the drawn polygon is the eave outline, and the
    # overhang only insets the walls inward — so the roof area and eave
    # perimeter are unchanged by the overhang.
    fp = Footprint.from_lists([(0, 0), (12, 0), (12, 8), (0, 8)], ["eave"] * 4)
    r0 = generate_roof(fp, 35, overhang=0.0)
    r1 = generate_roof(fp, 35, overhang=0.5)
    assert r1.total_area_m2 == pytest.approx(r0.total_area_m2)
    assert r1.eaves_perimeter_m == pytest.approx(40.0, abs=1e-6)  # 2*12 + 2*8


def test_overhang_lshape_unchanged():
    fp = Footprint.from_lists(
        [(0, 0), (6, 0), (6, 3), (3, 3), (3, 5), (0, 5)], ["eave"] * 6)
    r0 = generate_roof(fp, 35, overhang=0.0)
    r1 = generate_roof(fp, 35, overhang=0.4)
    assert r1.valley_count == 1
    assert r1.total_area_m2 == pytest.approx(r0.total_area_m2)


def test_gables_honoured_with_overhang():
    # New semantics bonus: overhang no longer rebuilds the roof all-eave, so a
    # gabled rectangle stays a gable roof at any overhang.
    fp = Footprint.from_lists(
        [(0, 0), (8, 0), (8, 5), (0, 5)], ["gable", "eave", "gable", "eave"])
    r = generate_roof(fp, 35, overhang=0.5)
    assert r.gable_edge_total_m > 0
    assert r.hip_count == 0
