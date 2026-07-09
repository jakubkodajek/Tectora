# -*- coding: utf-8 -*-
"""Pure tests for the Czech-locale annotation formatters (no Qt needed).

Segment classification and placement moved to the core (generate_roof emits
model-frame skeleton_segments for the exact path too) and are covered by
tests/core/test_exact_model_frame.py.
"""
from tectora.gui.editor.skeleton_render import fmt_area, fmt_len


def test_fmt_czech():
    assert fmt_len(4.0) == "4,00 m"
    assert fmt_area(4.62) == "4,62 m²"
