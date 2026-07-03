"""Material takeoff: quantities derived from roof measurements."""
import math

from tectora.core.footprint import Footprint
from tectora.core.materials import MaterialLine, MaterialSpec, material_takeoff, spec_note
from tectora.core.roof_generator import generate_roof

RECT = [(0, 0), (10, 0), (10, 6), (0, 6)]


def _hip_result(pitch=35.0):
    fp = Footprint.from_lists(RECT, ["eave"] * 4)
    return generate_roof(fp, pitch)


def _by_name(lines):
    return {ln.name: ln for ln in lines}


def test_tiles_use_area_waste_and_round_up():
    r = _hip_result()
    spec = MaterialSpec(tiles_per_m2=10.0, waste_pct=5.0)
    lines = _by_name(material_takeoff([r], spec))
    tiles = lines["Tašky (vč. prořezu 5 %)"]
    assert tiles.unit == "ks"
    assert tiles.qty == math.ceil(r.total_area_m2 * 10.0 * 1.05)


def test_ridge_caps_cover_ridge_plus_hips():
    r = _hip_result()
    spec = MaterialSpec(ridge_caps_per_m=2.5)
    lines = _by_name(material_takeoff([r], spec))
    caps = lines["Hřebenáče (hřeben + nároží)"]
    assert caps.qty == math.ceil((r.ridge_length_m + r.hip_length_m) * 2.5)


def test_hip_roof_has_no_gable_or_valley_rows():
    r = _hip_result()
    names = [ln.name for ln in material_takeoff([r])]
    assert not any("Krajové" in n for n in names)
    assert not any("Úžlabní" in n for n in names)


def test_eaves_flashing_is_linear_metres():
    r = _hip_result()
    lines = _by_name(material_takeoff([r]))
    assert lines["Okapní plech"].qty == round(r.eaves_perimeter_m, 2)
    assert lines["Okapní plech"].unit == "m"


def test_multiple_buildings_aggregate():
    r = _hip_result()
    one = _by_name(material_takeoff([r]))["Okapní plech"].qty
    two = _by_name(material_takeoff([r, r]))["Okapní plech"].qty
    assert two == round(one * 2, 2)


def test_spec_note_mentions_rates():
    note = spec_note(MaterialSpec(tiles_per_m2=12.5, waste_pct=8.0))
    assert "12.5" in note and "8" in note


def test_gabled_roof_gets_gable_tiles():
    fp = Footprint.from_lists(RECT, ["eave", "gable", "eave", "gable"])
    r = generate_roof(fp, 35.0)
    lines = _by_name(material_takeoff([r], MaterialSpec(gable_tiles_per_m=3.0)))
    assert lines["Krajové tašky"].qty == math.ceil(r.gable_edge_total_m * 3.0)
