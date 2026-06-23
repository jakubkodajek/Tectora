"""Verifikuje matematiku pro každý typ střechy proti známým vzorcům."""
import math

import pytest

from tectora.core import RoofSpec, RoofType, calculate


def approx(value, tol=1e-3):
    return pytest.approx(value, abs=tol)


# Společný invariant: plocha sklonové roviny × cos(α) = plocha jejího půdorysu.
# Test ploch dělám hlavně přes celkový součet = L*W/cos(α) pro typy s rovnoměrným sklonem.


class TestShed:
    def test_basic_area(self):
        spec = RoofSpec(RoofType.SHED, length=10, width=6, pitch=30)
        r = calculate(spec)
        expected = 10 * 6 / math.cos(math.radians(30))
        assert r.total_area_m2 == approx(expected)

    def test_no_ridge_no_hips(self):
        r = calculate(RoofSpec(RoofType.SHED, 10, 6, 30))
        assert r.ridge_length_m == 0
        assert r.hip_count == 0
        assert r.hip_length_m == 0

    def test_gable_edges(self):
        r = calculate(RoofSpec(RoofType.SHED, 10, 6, 30))
        slope = 6 / math.cos(math.radians(30))
        assert r.gable_edge_left_m == approx(slope)
        assert r.gable_edge_right_m == approx(slope)

    def test_eaves(self):
        r = calculate(RoofSpec(RoofType.SHED, 10, 6, 30))
        assert r.eaves_perimeter_m == approx(20)

    def test_face_count(self):
        r = calculate(RoofSpec(RoofType.SHED, 10, 6, 30))
        assert len(r.faces) == 1


class TestGable:
    def test_total_area_symmetric(self):
        r = calculate(RoofSpec(RoofType.GABLE, 10, 6, 30))
        expected = 10 * 6 / math.cos(math.radians(30))
        assert r.total_area_m2 == approx(expected)

    def test_ridge_length(self):
        r = calculate(RoofSpec(RoofType.GABLE, 12, 8, 25))
        assert r.ridge_length_m == approx(12)

    def test_apex_height_symmetric(self):
        r = calculate(RoofSpec(RoofType.GABLE, 10, 6, 30))
        assert r.apex_height_m == approx(3 * math.tan(math.radians(30)))

    def test_asymmetric_apex_height_continuity(self):
        # Pokud pitch_back == pitch, výsledek musí být stejný jako symetrický
        sym = calculate(RoofSpec(RoofType.GABLE, 10, 6, 30))
        asym = calculate(RoofSpec(RoofType.GABLE, 10, 6, 30, pitch_back=30))
        assert sym.apex_height_m == approx(asym.apex_height_m)
        assert sym.total_area_m2 == approx(asym.total_area_m2)

    def test_asymmetric_apex_height(self):
        # Strmější přední (60°), pologé zadní (20°)
        r = calculate(RoofSpec(RoofType.GABLE, 10, 6, 60, pitch_back=20))
        # h = y_apex * tan(60) = (6 - y_apex) * tan(20)
        # y_apex = 6 * tan(20)/(tan(60)+tan(20))
        y_apex = 6 * math.tan(math.radians(20)) / (
            math.tan(math.radians(60)) + math.tan(math.radians(20))
        )
        assert r.apex_height_m == approx(y_apex * math.tan(math.radians(60)))

    def test_gable_edges(self):
        r = calculate(RoofSpec(RoofType.GABLE, 10, 6, 30))
        # Dvě šikmé hrany na štítu, každá slope_len
        slope = 3 / math.cos(math.radians(30))
        assert r.gable_edge_left_m == approx(2 * slope)

    def test_no_hips_no_valleys(self):
        r = calculate(RoofSpec(RoofType.GABLE, 10, 6, 30))
        assert r.hip_count == 0
        assert r.hip_length_m == 0
        assert r.valley_length_m == 0


class TestHip:
    def test_total_area(self):
        r = calculate(RoofSpec(RoofType.HIP, 10, 6, 30))
        expected = 10 * 6 / math.cos(math.radians(30))
        assert r.total_area_m2 == approx(expected)

    def test_ridge_length(self):
        r = calculate(RoofSpec(RoofType.HIP, 10, 6, 30))
        assert r.ridge_length_m == approx(10 - 6)

    def test_four_hips(self):
        r = calculate(RoofSpec(RoofType.HIP, 10, 6, 30))
        assert r.hip_count == 4
        h = 3 * math.tan(math.radians(30))
        hip = math.sqrt(9 + 9 + h * h)
        assert r.hip_length_m == approx(4 * hip)

    def test_no_gable_edges(self):
        r = calculate(RoofSpec(RoofType.HIP, 10, 6, 30))
        assert r.gable_edge_left_m == 0
        assert r.gable_edge_right_m == 0

    def test_eaves_full_perimeter(self):
        r = calculate(RoofSpec(RoofType.HIP, 10, 6, 30))
        assert r.eaves_perimeter_m == approx(2 * 10 + 2 * 6)

    def test_four_faces(self):
        r = calculate(RoofSpec(RoofType.HIP, 10, 6, 30))
        assert len(r.faces) == 4


class TestHalfHip:
    def test_ratio_zero_equals_gable(self):
        gable = calculate(RoofSpec(RoofType.GABLE, 10, 6, 30))
        half = calculate(RoofSpec(RoofType.HALF_HIP, 10, 6, 30, half_hip_ratio=0))
        assert half.total_area_m2 == approx(gable.total_area_m2)
        assert half.ridge_length_m == approx(gable.ridge_length_m)

    def test_ratio_one_equals_hip(self):
        hip = calculate(RoofSpec(RoofType.HIP, 10, 6, 30))
        half = calculate(RoofSpec(RoofType.HALF_HIP, 10, 6, 30, half_hip_ratio=1))
        assert half.total_area_m2 == approx(hip.total_area_m2)
        assert half.ridge_length_m == approx(hip.ridge_length_m)

    def test_ridge_shrinks_with_ratio(self):
        r0 = calculate(RoofSpec(RoofType.HALF_HIP, 10, 6, 30, half_hip_ratio=0.3))
        r1 = calculate(RoofSpec(RoofType.HALF_HIP, 10, 6, 30, half_hip_ratio=0.7))
        assert r0.ridge_length_m > r1.ridge_length_m

    def test_total_area_intermediate(self):
        gable = calculate(RoofSpec(RoofType.GABLE, 10, 6, 30))
        hip = calculate(RoofSpec(RoofType.HIP, 10, 6, 30))
        half = calculate(RoofSpec(RoofType.HALF_HIP, 10, 6, 30, half_hip_ratio=0.5))
        # Polovalbová má strmější konce — víc plochy než sedlová.
        # Stejnou nebo víc než valbová (záleží na ratio)
        assert half.total_area_m2 >= gable.total_area_m2 - 1e-6

    def test_intermediate_ratio_full(self):
        # Pro ratio 0.5, L=10, W=6, pitch=30:
        # d=1.5, h=√3, slope_len=2√3
        # trapezoid_area = (10+7)/2 × 2√3 = 17√3
        # end_slant = √(1.5² + 3) = √5.25
        # triangle_area = 3 × √5.25
        # total = 2 × 17√3 + 2 × 3√5.25 = 34√3 + 6√5.25
        r = calculate(RoofSpec(RoofType.HALF_HIP, 10, 6, 30, half_hip_ratio=0.5))
        expected = 34 * math.sqrt(3) + 6 * math.sqrt(5.25)
        assert r.total_area_m2 == approx(expected)


class TestPyramid:
    def test_total_area(self):
        r = calculate(RoofSpec(RoofType.PYRAMID, 6, 6, 30))
        expected = 36 / math.cos(math.radians(30))
        assert r.total_area_m2 == approx(expected)

    def test_no_ridge(self):
        r = calculate(RoofSpec(RoofType.PYRAMID, 6, 6, 30))
        assert r.ridge_length_m == 0

    def test_four_hips(self):
        r = calculate(RoofSpec(RoofType.PYRAMID, 6, 6, 30))
        assert r.hip_count == 4
        h = 3 * math.tan(math.radians(30))
        expected_hip = math.sqrt(9 + 9 + h * h)
        assert r.hip_length_m == approx(4 * expected_hip)

    def test_non_square_rejected(self):
        with pytest.raises(ValueError, match="čtvercový"):
            calculate(RoofSpec(RoofType.PYRAMID, 10, 6, 30))

    def test_four_faces(self):
        r = calculate(RoofSpec(RoofType.PYRAMID, 6, 6, 30))
        assert len(r.faces) == 4


class TestValidation:
    def test_negative_length(self):
        with pytest.raises(ValueError, match="length"):
            calculate(RoofSpec(RoofType.GABLE, -1, 6, 30))

    def test_pitch_out_of_range(self):
        with pytest.raises(ValueError, match="pitch"):
            calculate(RoofSpec(RoofType.GABLE, 10, 6, 0))
        with pytest.raises(ValueError, match="pitch"):
            calculate(RoofSpec(RoofType.GABLE, 10, 6, 90))

    def test_half_hip_too_aggressive(self):
        # Příliš krátká budova pro plnou valbu
        with pytest.raises(ValueError, match="hřeben"):
            calculate(RoofSpec(RoofType.HALF_HIP, length=4, width=8, pitch=30, half_hip_ratio=1))


class TestEaveOverhang:
    def test_overhang_increases_area(self):
        no_overhang = calculate(RoofSpec(RoofType.GABLE, 10, 6, 30))
        with_overhang = calculate(RoofSpec(RoofType.GABLE, 10, 6, 30, eave_overhang=0.5))
        assert with_overhang.total_area_m2 > no_overhang.total_area_m2

    def test_overhang_extends_ridge(self):
        with_overhang = calculate(RoofSpec(RoofType.GABLE, 10, 6, 30, eave_overhang=0.5))
        # Hřeben sedlové = L + 2*overhang
        assert with_overhang.ridge_length_m == approx(11)


class TestAsDict:
    def test_round_trip_keys(self):
        r = calculate(RoofSpec(RoofType.GABLE, 10, 6, 30, name="testovací projekt"))
        d = r.as_dict()
        assert d["project_name"] == "testovací projekt"
        assert d["input"]["type_cs"] == "sedlová"
        assert "total_area_m2" in d["results"]
        assert d["results"]["ridge_length_m"] == approx(10)
