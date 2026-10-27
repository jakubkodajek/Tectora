"""Reader for DACAD project files (``.DAD``).

DACAD saves the roof *input* (footprint walls with their roof definitions,
windows, chimneys) together with the *results* it computed (roof lines with
type and length, roof faces with area, and the per-type group totals). That
makes every saved ``.DAD`` a golden test case: feed the input to Tectora and
compare against the numbers DACAD stored.

Two on-disk formats exist (both reverse-engineered from sample files; field
meanings were confirmed by cross-checking per-line sums against the stored
group totals):

* **V1** (DACAD 1.1x, 1994–96): header ``b"DACAD\\x1a"``, then flat records
  ``>H tag, <H length, payload``.
* **V2** (DACAD 2.x, 1997+): header ``b"\\x08DACADV2\\x1a"``, then a tagged
  tree: ``FF FF <u16 field id> FF FF FF <u32 length> <payload>``. A list
  payload is ``<u16 count>`` followed by ``count`` objects, each
  ``<u16 object id>``, its fields, and a ``0x00`` terminator.

Only what the verification needs is decoded; unknown fields are ignored.
"""
from __future__ import annotations

import math
import struct
from dataclasses import dataclass, field, replace
from pathlib import Path

# DACAD line-type codes (identical in V1 and V2) → DACAD's own abbreviations
# from the help file (Linienbeschriftung).
LINE_TYPES: dict[int, str] = {
    1: "T",    # Traufe — eave
    2: "F",    # First — ridge
    3: "G",    # Grat — hip
    4: "K",    # Kehle — valley
    5: "P",    # Pult — top edge of a shed roof
    6: "OL",   # Ortgang links — left verge / rake
    7: "OR",   # Ortgang rechts — right verge / rake
    8: "OS",   # Ortgang schräg — oblique verge
    9: "WV",   # Wandanschluss vorne — front wall abutment
    10: "WH",  # Wandanschluss hinten — back wall abutment
    11: "WS",  # Wandanschluss seitlich — side wall abutment
    12: "c",   # Knick — pitch break
    15: "S",   # Schlucht — gorge
}

# Line types that DACAD sums into "Linien gesamt" (all real roof lines).
ROOF_LINE_TYPES = frozenset(range(1, 12)) | {12, 15}


@dataclass
class RoofDefinition:
    """One roof definition (Dachdefinition) attached to a wall.

    A wall carries one definition, or several stacked ones (e.g. a vertical
    gable up to the Krüppelwalm height, then a hip plane above it).
    """
    pitch_deg: float
    eave_height_m: float
    overhang_m: float
    material: int          # 0 = none (gable), 1 = tiles (Ziegel), 2 = sheet (Blech)
    roof_no: int = 1
    # Horizontal priority: decides between two possible horizontal
    # intersection lines; without one DACAD takes the line at the steeper face.
    priority: int = 0


@dataclass
class Wall:
    start: tuple[float, float]
    end: tuple[float, float]
    definitions: list[RoofDefinition] = field(default_factory=list)
    # Stored wall direction. Needed for zero-length walls, e.g. the side
    # walls of a dormer, whose direction alone orients their plane.
    direction_deg: float | None = None

    @property
    def length(self) -> float:
        return math.dist(self.start, self.end)


@dataclass
class Window:
    """Roof window: x, y in plan; width and height measured in the roof plane."""
    x: float
    y: float
    width: float
    height: float


@dataclass
class Chimney:
    height_m: float
    corners: list[tuple[float, float]]  # plan polygon


@dataclass
class ResultLine:
    type_code: int
    length_m: float
    a: tuple[float, float, float] | None = None
    b: tuple[float, float, float] | None = None

    @property
    def type(self) -> str:
        return LINE_TYPES.get(self.type_code, f"?{self.type_code}")


@dataclass
class DadProject:
    path: str
    version: str
    walls: list[Wall]
    lines: list[ResultLine]
    face_areas: list[float]
    line_groups: dict[str, float]   # DACAD group name → stored total, m
    area_groups: dict[str, float]   # DACAD group name → stored total, m²
    # Openings are decoded from V1 files only so far (V2 stores windows
    # relative to the roof face).
    windows: list[Window] = field(default_factory=list)
    chimneys: list[Chimney] = field(default_factory=list)

    def line_totals(self) -> dict[str, float]:
        """Sum of result line lengths per DACAD type abbreviation."""
        totals: dict[str, float] = {}
        for ln in self.lines:
            if ln.type_code in LINE_TYPES:
                totals[ln.type] = totals.get(ln.type, 0.0) + ln.length_m
        return totals


def read_dad(path: str | Path) -> DadProject:
    data = Path(path).read_bytes()
    if data.startswith(b"DACAD\x1a"):
        return _read_v1(str(path), data)
    if data.startswith(b"\x08DACADV2\x1a"):
        return _read_v2(str(path), data)
    raise ValueError(f"Not a DACAD .DAD file: {path}")


# --------------------------------------------------------------------- V1 ---

def _v1_records(data: bytes):
    i = 6
    while i + 4 <= len(data):
        tag = struct.unpack_from(">H", data, i)[0]
        ln = struct.unpack_from("<H", data, i + 2)[0]
        yield tag, data[i + 4:i + 4 + ln]
        if tag == 0xFFFF:
            return
        i += 4 + ln


def _read_v1(path: str, data: bytes) -> DadProject:
    walls: list[Wall] = []
    lines: list[ResultLine] = []
    faces: list[float] = []
    line_groups: dict[str, float] = {}
    area_groups: dict[str, float] = {}
    windows: list[Window] = []
    chimneys: list[Chimney] = []
    version = "V1"
    for tag, d in _v1_records(data):
        if tag == 0x0001:
            version = d.split(b"\0")[0].decode("latin-1")
        elif tag == 0x0012:
            # id, roof no, x, y, length, direction (rad), ?, material,
            # eave height, overhang, pitch (deg), priority (f32), knick count,
            # window count
            _id, roof_no = struct.unpack_from("<HH", d, 0)
            x, y, length, ang = struct.unpack_from("<4f", d, 4)
            eave_h, overhang, pitch, prio = struct.unpack_from("<4f", d, 24)
            end = (x + length * math.cos(ang), y + length * math.sin(ang))
            material = 0 if abs(pitch - 90.0) < 1e-6 else 1
            walls.append(Wall((x, y), end, [RoofDefinition(
                pitch, eave_h, overhang, material, roof_no, int(round(prio)))],
                math.degrees(ang)))
        elif tag == 0x1012 and walls:
            # Stacked definition above the previous one (Krüppelwalm / knick):
            # ?, ?, eave height, overhang, pitch
            eave_h, overhang, pitch = struct.unpack_from("<3f", d, 4)
            prev = walls[-1].definitions[-1]
            walls[-1].definitions.append(RoofDefinition(
                pitch, eave_h, overhang, 1, prev.roof_no))
        elif tag == 0x2012:
            windows.append(Window(*struct.unpack_from("<4f", d, 0)))
        elif tag == 0x0013:
            _id, height = struct.unpack_from("<Hf", d, 0)
            chimneys.append(Chimney(height, []))
        elif tag == 0x1013 and chimneys:
            chimneys[-1].corners.append(struct.unpack_from("<2f", d, 0))
        elif tag == 0x1021:
            a = struct.unpack_from("<3f", d, 2)
            b = struct.unpack_from("<3f", d, 14)
            t = struct.unpack_from("<H", d, 26)[0]
            length = struct.unpack_from("<f", d, 28)[0]
            lines.append(ResultLine(t, length, a, b))
        elif tag == 0x2021:
            faces.append(struct.unpack_from("<f", d, 5)[0])
        elif tag == 0x1022:
            name = d[:12].split(b"\0")[0].decode("cp1252")
            line_groups[name] = struct.unpack_from("<f", d, len(d) - 4)[0]
        elif tag == 0x2022:
            name = d[:14].split(b"\0")[0].decode("cp1252")
            area_groups[name] = struct.unpack_from("<f", d, len(d) - 6)[0]
    return DadProject(path, version, walls, lines, faces, line_groups, area_groups,
                      windows, chimneys)


# --------------------------------------------------------------------- V2 ---

def _is_tlv(data: bytes, i: int) -> bool:
    return (i + 11 <= len(data) and data[i] == 0xFF and data[i + 1] == 0xFF
            and data[i + 4:i + 7] == b"\xff\xff\xff")


def _fields(data: bytes, i: int, end: int) -> tuple[dict[int, bytes], int]:
    """Parse TLV fields from *i* until a 0x00 terminator (or *end*)."""
    out: dict[int, bytes] = {}
    while i < end and _is_tlv(data, i):
        fid = struct.unpack_from("<H", data, i + 2)[0]
        ln = struct.unpack_from("<I", data, i + 7)[0]
        out.setdefault(fid, data[i + 11:i + 11 + ln])
        i += 11 + ln
    if i < end and data[i] == 0x00:
        i += 1
    return out, i


def _objects(payload: bytes) -> list[tuple[int, dict[int, bytes]]]:
    """Decode a list payload: u16 count, then (u16 id, fields, 0x00)*count."""
    if len(payload) < 2:
        return []
    count = struct.unpack_from("<H", payload, 0)[0]
    i = 2
    out = []
    for _ in range(count):
        if i + 2 > len(payload):
            break
        oid = struct.unpack_from("<H", payload, i)[0]
        flds, i = _fields(payload, i + 2, len(payload))
        out.append((oid, flds))
    return out


def _f32(b: bytes | None) -> float:
    return struct.unpack("<f", b)[0] if b and len(b) == 4 else 0.0


def _u(b: bytes | None) -> int:
    if not b:
        return 0
    return int.from_bytes(b[:4], "little")


def _vec(b: bytes) -> tuple[float, float, float]:
    """Decode a tagged vector: ``(tag, f32)*`` then 0x00.

    The tag's high nibble is the component (1 = x, 2 = y, 3 = z) and its low
    nibble the payload size; components that are absent are zero.
    """
    comps = [0.0, 0.0, 0.0]
    i = 0
    while i < len(b) and b[i] != 0:
        tag = b[i]
        size = tag & 0x0F
        k = (tag >> 4) - 1
        if size == 4 and 0 <= k < 3 and i + 5 <= len(b):
            comps[k] = struct.unpack_from("<f", b, i + 1)[0]
        i += 1 + size
    return (comps[0], comps[1], comps[2])


def _pt2(b: bytes) -> tuple[float, float]:
    x, y, _ = _vec(b)
    return (x, y)


def _pt3(b: bytes) -> tuple[float, float, float]:
    return _vec(b)


def _pstr(b: bytes) -> str:
    # Pascal-style: 1 length byte, then cp1252 text.
    return b[1:1 + b[0]].decode("cp1252") if b else ""


def _id_lists(payload: bytes) -> dict[int, list[int]]:
    """Decode ``u16 count, (u16 key, u16 n, u16 ids[n])*count``."""
    out: dict[int, list[int]] = {}
    if len(payload) < 2:
        return out
    count = struct.unpack_from("<H", payload, 0)[0]
    i = 2
    for _ in range(count):
        key, n = struct.unpack_from("<HH", payload, i)
        out[key] = list(struct.unpack_from(f"<{n}H", payload, i + 4))
        i += 4 + 2 * n
    return out


def _read_v2(path: str, data: bytes) -> DadProject:
    top, _ = _fields(data, 9, len(data))
    proj_payload = top.get(1, b"")
    proj, _ = _fields(proj_payload, 0, len(proj_payload))

    # [6] roof definitions: [1] roof no, [2] pitch, [3] eave height,
    # [4] overhang, [5] material, [6] priority.
    defs: dict[int, RoofDefinition] = {}
    for oid, f in _objects(proj.get(6, b"")):
        defs[oid] = RoofDefinition(
            pitch_deg=_f32(f.get(2)), eave_height_m=_f32(f.get(3)),
            overhang_m=_f32(f.get(4)), material=_u(f.get(5)),
            roof_no=_u(f.get(1)), priority=_u(f.get(6)))

    # [100][1][2]: wall id → its definition ids (bottom to top).
    rel_payload = proj.get(100, b"")
    rel, _ = _fields(rel_payload, 0, len(rel_payload))
    rel1, _ = _fields(rel.get(1, b""), 0, len(rel.get(1, b"")))
    wall_defs = _id_lists(rel1.get(2, b""))
    # [100][1][1]: footprint loops (ordered wall ids per roof).
    loops = _id_lists(rel1.get(1, b""))

    # [4] roofs: [1] roof no, [2] third value of V1's 0x11 record. V1's
    # second value (dormer flag) has no V2 field found yet.
    # [5] walls: [2] geometry {[1] start, [2] end, [3] length,
    # [4] direction (rad)}. Same orientation and order as V1.
    walls_by_id: dict[int, Wall] = {}
    for oid, f in _objects(proj.get(5, b"")):
        geo, _ = _fields(f.get(2, b""), 0, len(f.get(2, b"")))
        if 1 not in geo or 2 not in geo:
            continue
        walls_by_id[oid] = Wall(
            _pt2(geo[1]), _pt2(geo[2]),
            [defs[d] for d in wall_defs.get(oid, []) if d in defs],
            math.degrees(_f32(geo[4])) if 4 in geo else None)
    # The loop key is the roof number; the definitions' own roof field is
    # not reliable in files DACAD saved.
    ordered = [replace(walls_by_id[w], definitions=[
                   replace(d, roof_no=roof) for d in walls_by_id[w].definitions])
               for roof, ids in sorted(loops.items()) for w in ids
               if w in walls_by_id]
    walls = ordered or list(walls_by_id.values())

    # [7] result lines: [2] {[2] a, [1] b}, [3] length, [5] type code.
    lines: list[ResultLine] = []
    for _oid, f in _objects(proj.get(7, b"")):
        ends, _ = _fields(f.get(2, b""), 0, len(f.get(2, b"")))
        a = _pt3(ends[2]) if 2 in ends else None
        b = _pt3(ends[1]) if 1 in ends else None
        lines.append(ResultLine(_u(f.get(5)), _f32(f.get(3)), a, b))

    # [8] result faces: [1] area.
    faces = [_f32(f.get(1)) for _oid, f in _objects(proj.get(8, b""))]

    # [9] line groups, [10] area groups: [2] name, [6] total.
    line_groups = {_pstr(f.get(2, b"")): _f32(f.get(6))
                   for _oid, f in _objects(proj.get(9, b""))}
    area_groups = {_pstr(f.get(2, b"")): _f32(f.get(6))
                   for _oid, f in _objects(proj.get(10, b""))}
    return DadProject(path, "V2", walls, lines, faces, line_groups, area_groups)
