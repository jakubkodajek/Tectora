import math
import pytest
from tectora.core.footprint import Footprint
from tectora.core.roof_faces import build_faces

SHAPES = {
    "rect": [(0, 0), (12, 0), (12, 8), (0, 8)],
    "L": [(0, 0), (6, 0), (6, 3), (3, 3), (3, 5), (0, 5)],
    "U": [(0, 0), (7, 0), (7, 5), (5, 5), (5, 2), (2, 2), (2, 5), (0, 5)],
    "T": [(0, 4), (3, 4), (3, 0), (6, 0), (6, 4), (9, 4), (9, 7), (0, 7)],
    "plus": [(2, 0), (4, 0), (4, 2), (6, 2), (6, 4), (4, 4),
             (4, 6), (2, 6), (2, 4), (0, 4), (0, 2), (2, 2)],
}


def _fp_area(poly):
    n = len(poly)
    return abs(sum(poly[i][0]*poly[(i+1) % n][1] - poly[(i+1) % n][0]*poly[i][1]
                   for i in range(n))) / 2


@pytest.mark.parametrize("name", list(SHAPES))
def test_one_face_per_edge_and_areas_sum(name):
    poly = SHAPES[name]
    faces = build_faces(Footprint.from_lists(poly, ["eave"] * len(poly)), 35)
    # one roof plane per polygon edge
    assert len(faces) == len(poly), (name, len(faces))
    # sum of 3D face areas == footprint_area / cos(pitch)
    expected = _fp_area(poly) / math.cos(math.radians(35))
    assert sum(f.area_m2 for f in faces) == pytest.approx(expected, rel=1e-6)
    # each face is a planar polygon with >=3 vertices, all area>0
    for f in faces:
        assert len(f.vertices) >= 3
        assert f.area_m2 > 0
