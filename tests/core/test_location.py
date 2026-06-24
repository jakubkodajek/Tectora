from tectora.core.location import Location


def test_location_defaults_have_no_coords():
    loc = Location(name="Brno")
    assert loc.name == "Brno"
    assert loc.lat is None
    assert loc.lon is None


def test_location_with_coords():
    loc = Location("Moravský Krumlov", 49.048, 16.310)
    assert loc.lat == 49.048
    assert loc.lon == 16.310
