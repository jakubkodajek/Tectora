from tectora.io.nominatim_geocoder import NominatimGeocoder


def fake_fetch_ok(query, limit):
    return [
        {"display_name": "Moravský Krumlov, Česko", "lat": "49.05", "lon": "16.31"},
        {"display_name": "Moravské Budějovice, Česko", "lat": "49.05", "lon": "15.81"},
    ]


def fake_fetch_offline(query, limit):
    raise OSError("offline")


def test_search_maps_results():
    geo = NominatimGeocoder(fetch=fake_fetch_ok)
    out = geo.search("Mor", limit=5)
    assert len(out) == 2
    assert out[0].name.startswith("Moravský Krumlov")
    assert out[0].lat == 49.05


def test_search_empty_query_returns_empty():
    geo = NominatimGeocoder(fetch=fake_fetch_ok)
    assert geo.search("   ") == []


def test_search_offline_returns_empty_without_raising():
    geo = NominatimGeocoder(fetch=fake_fetch_offline)
    assert geo.search("Mor") == []
