"""DACAD .DAD reader.

The synthetic tests pin the record layouts. The corpus test runs only when a
local DACAD sample folder is given via ``TECTORA_DACAD_DIR`` (the sample files
are proprietary and not committed); it checks that the decoded result lines
add up to the group totals DACAD stored — the evidence that the line type and
length fields are decoded correctly.
"""
import math
import os
import struct
from pathlib import Path

import pytest

from tectora.io.dacad_dad import read_dad


def _v1_record(tag: int, payload: bytes) -> bytes:
    return struct.pack(">H", tag) + struct.pack("<H", len(payload)) + payload


def _v1_wall(wid: int, x: float, y: float, length: float, ang: float,
             eave_h: float, overhang: float, pitch: float) -> bytes:
    material = 0 if pitch == 90.0 else 1  # gable: material "kein"
    return (struct.pack("<HH4fHH3ff", wid, 1, x, y, length, ang, 0, material,
                        eave_h, overhang, pitch, 0.0) + b"\0\0")


def _v1_line(lid: int, a, b, type_code: int, length: float) -> bytes:
    return struct.pack("<H3f3fHff", lid, *a, *b, type_code, length, length)


def test_v1_walls_lines_and_groups(tmp_path: Path):
    data = b"DACAD\x1a" + b"".join([
        _v1_record(0x0001, b"DACAD V1.13\0"),
        _v1_record(0x0012, _v1_wall(1, 0, 0, 10, 0, 3.0, 0.5, 40.0)),
        _v1_record(0x0012, _v1_wall(2, 10, 0, 6, math.pi / 2, 3.0, 0.5, 90.0)),
        _v1_record(0x1012, struct.pack("<HH3f", 0, 1, 5.5, 0.0, 45.0)),
        _v1_record(0x1021, _v1_line(1, (0, 0, 3), (10, 0, 3), 1, 10.0)),
        _v1_record(0x1021, _v1_line(2, (2, 3, 6), (8, 3, 6), 2, 6.0)),
        _v1_record(0x1022, b"Traufen\0\0\0\0\0" + bytes(9) + struct.pack("<f", 10.0)),
        _v1_record(0xFFFF, b""),
    ])
    p = tmp_path / "x.dad"
    p.write_bytes(data)

    pr = read_dad(p)

    assert pr.version == "DACAD V1.13"
    assert len(pr.walls) == 2
    w1, w2 = pr.walls
    assert w1.start == (0, 0) and w1.end == pytest.approx((10, 0))
    assert w1.definitions[0].pitch_deg == pytest.approx(40.0)
    assert w1.definitions[0].overhang_m == pytest.approx(0.5)
    assert w2.end == pytest.approx((10, 6), abs=1e-5)
    # 90° wall = gable (no material); the stacked record is the Krüppelwalm.
    assert w2.definitions[0].material == 0
    assert w2.definitions[1].pitch_deg == pytest.approx(45.0)
    assert w2.definitions[1].eave_height_m == pytest.approx(5.5)
    assert pr.line_totals() == {"T": 10.0, "F": 6.0}
    assert pr.line_groups == {"Traufen": 10.0}


def test_rejects_other_files(tmp_path: Path):
    p = tmp_path / "x.dad"
    p.write_bytes(b"not a dacad file")
    with pytest.raises(ValueError):
        read_dad(p)


_GROUP_TYPES = {
    "Traufe": ["T"], "Traufen": ["T"], "First": ["F"], "Firste": ["F"],
    "Grat": ["G"], "Grate": ["G"], "Kehle": ["K"], "Kehlen": ["K"],
    "Pult": ["P"], "Pulte": ["P"], "Ortgang links": ["OL"],
    "Ortgang rechts": ["OR"], "Ortgang schräg": ["OS"],
    "Ortgänge": ["OL", "OR", "OS"], "Wandanschluß vorne": ["WV"],
    "Wandanschluß hinten": ["WH"], "Wandanschluß seitlich": ["WS"],
    "Wandanschlüs": ["WV", "WH", "WS"], "Schlucht": ["S"],
}

_CORPUS = os.environ.get("TECTORA_DACAD_DIR")


@pytest.mark.skipif(not _CORPUS, reason="set TECTORA_DACAD_DIR to a DACAD sample folder")
def test_corpus_lines_add_up_to_stored_group_totals():
    files = sorted(p for p in Path(_CORPUS).iterdir() if p.suffix.lower() == ".dad")
    assert files
    for p in files:
        pr = read_dad(p)
        assert pr.walls, p.name
        totals = pr.line_totals()
        for group, value in pr.line_groups.items():
            types = _GROUP_TYPES.get(group)
            if types is None:
                continue
            got = sum(totals.get(t, 0.0) for t in types)
            assert got == pytest.approx(value, abs=0.01), f"{p.name}: {group}"
