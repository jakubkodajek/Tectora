import pytest
from tectora.core.footprint import Footprint
from tectora.gui.home.project_card import footprint_to_mini_points


def test_maps_into_padded_box():
    fp = Footprint.from_lists([(0, 0), (12, 0), (12, 8), (0, 8)],
                              ["eave", "gable", "eave", "gable"])
    pts = footprint_to_mini_points(fp, 200, 120, pad=10)
    assert len(pts) == 4
    xs = [p[0] for p in pts]
    ys = [p[1] for p in pts]
    assert min(xs) >= 10 - 1e-6 and max(xs) <= 190 + 1e-6
    assert min(ys) >= 10 - 1e-6 and max(ys) <= 110 + 1e-6
