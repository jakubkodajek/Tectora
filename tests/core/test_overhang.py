import math
import pytest
from tectora.core.footprint import Footprint
from tectora.core.roof_generator import generate_roof


def test_overhang_increases_area_and_eave():
    fp = Footprint.from_lists([(0, 0), (12, 0), (12, 8), (0, 8)], ["eave"] * 4)
    r0 = generate_roof(fp, 35, overhang=0.0)
    r1 = generate_roof(fp, 35, overhang=0.5)
    assert r1.total_area_m2 > r0.total_area_m2
    # roof footprint 13 x 9 → eave perimeter 2*13+2*9 = 44
    assert r1.eaves_perimeter_m == pytest.approx(44.0, abs=1e-6)


def test_overhang_lshape_still_valid():
    fp = Footprint.from_lists(
        [(0, 0), (6, 0), (6, 3), (3, 3), (3, 5), (0, 5)], ["eave"] * 6)
    r = generate_roof(fp, 35, overhang=0.4)
    assert r.valley_count == 1
    assert r.total_area_m2 > 0
