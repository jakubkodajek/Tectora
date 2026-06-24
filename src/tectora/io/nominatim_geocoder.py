"""Adapter: place autocomplete via OpenStreetMap / Nominatim."""
from __future__ import annotations

import json
import urllib.parse
import urllib.request
from typing import Callable, Optional

from tectora.core.location import Location

_URL = "https://nominatim.openstreetmap.org/search"
_USER_AGENT = "Tectora/0.1 (roof calculator)"

FetchFn = Callable[[str, int], list]


class NominatimGeocoder:
    def __init__(self, fetch: Optional[FetchFn] = None, timeout: float = 5.0):
        self._fetch = fetch or self._http_fetch
        self._timeout = timeout

    def search(self, query: str, limit: int = 5) -> list[Location]:
        if not query.strip():
            return []
        try:
            raw = self._fetch(query, limit)
        except Exception:
            return []  # offline / timeout / error → empty, never raise
        results = []
        for item in raw or []:
            try:
                results.append(Location(
                    name=item["display_name"],
                    lat=float(item["lat"]),
                    lon=float(item["lon"]),
                ))
            except (KeyError, TypeError, ValueError):
                continue
        return results

    def _http_fetch(self, query: str, limit: int) -> list:
        params = urllib.parse.urlencode(
            {"q": query, "format": "json", "limit": limit})
        req = urllib.request.Request(
            f"{_URL}?{params}", headers={"User-Agent": _USER_AGENT})
        with urllib.request.urlopen(req, timeout=self._timeout) as resp:
            return json.loads(resp.read().decode("utf-8"))
