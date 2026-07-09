# -*- coding: utf-8 -*-
"""Pure (no-Qt) helpers for rendering roof annotations on the 2D canvas.

Historically this module also mapped exact-calculator results from the
calculator frame into the model frame (``map_calc_point`` and friends). That
responsibility moved into ``core.roof_generator``, which now emits every
RoofResult in model coordinates — so only the Czech-locale formatters remain.

Public functions:

  fmt_len(m) / fmt_area(m2)
      Czech-locale number formatters.
"""
from __future__ import annotations


def fmt_len(m: float) -> str:
    """Format a length in metres with Czech comma decimal, e.g. ``"4,00 m"``."""
    return f"{m:.2f}".replace(".", ",") + " m"


def fmt_area(m2: float) -> str:
    """Format an area in m² with Czech comma decimal, e.g. ``"4,62 m²"``."""
    return f"{m2:.2f}".replace(".", ",") + " m²"
