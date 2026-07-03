"""CLI interface for Tectora.

Usage examples:

    tectora calculate --type gable --length 10 --width 6 --pitch 30
    tectora calculate --spec roof.json --output result.json
    tectora calculate --spec input.xlsx --output report.xlsx
    tectora calculate --project dum.tectora --output vykaz.xlsx
    tectora validate dum.tectora         # check a project file before use
    tectora template --output input.xlsx
    tectora schema                       # JSON schema of the parametric spec
    tectora schema --project             # JSON schema of a project (polygons)
    tectora types                        # list of supported roof types

For AI: `tectora schema --project` describes the polygon-based project format
(the same one the GUI edits); the full workflow lives in docs/ai/.
"""
from __future__ import annotations

import argparse
import io
import json
import sys
from pathlib import Path

# Force UTF-8 stdout/stderr — on Windows the default is cp1250, which mangles diacritics.
if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8")  # type: ignore[attr-defined]
    sys.stderr.reconfigure(encoding="utf-8")  # type: ignore[attr-defined]
else:  # older Python — wrap buffer
    sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding="utf-8")
    sys.stderr = io.TextIOWrapper(sys.stderr.buffer, encoding="utf-8")

from tectora import __version__
from tectora.application import use_cases as uc
from tectora.application.library import EntryNotFound, entry_summary
from tectora.application.project import Project
from tectora.core import RoofSpec, RoofType, calculate
from tectora.core.location import Location
from tectora.core.result import RoofResult
from tectora.io import (
    read_spec_json,
    read_spec_xlsx,
    write_report_xlsx,
    write_result_json,
    write_template_xlsx,
)


_JSON_SCHEMA = {
    "$schema": "http://json-schema.org/draft-07/schema#",
    "title": "Tectora roof specification",
    "type": "object",
    "required": ["type", "length", "width", "pitch"],
    "properties": {
        "type": {
            "type": "string",
            "enum": ["shed", "gable", "hip", "half_hip", "pyramid"],
            "description": (
                "Typ střechy. shed=pultová, gable=sedlová, hip=valbová, "
                "half_hip=polovalbová, pyramid=stanová (vyžaduje čtvercový půdorys)."
            ),
        },
        "length": {
            "type": "number", "exclusiveMinimum": 0,
            "description": "Délka půdorysu (delší rozměr), v metrech.",
        },
        "width": {
            "type": "number", "exclusiveMinimum": 0,
            "description": "Šířka půdorysu (kratší rozměr), v metrech.",
        },
        "pitch": {
            "type": "number", "exclusiveMinimum": 0, "exclusiveMaximum": 90,
            "description": "Hlavní sklon ve stupních (1–89).",
        },
        "pitch_back": {
            "type": ["number", "null"], "exclusiveMinimum": 0, "exclusiveMaximum": 90,
            "description": "Sklon zadní strany pro asymetrickou sedlovou. null=symetrické.",
        },
        "half_hip_ratio": {
            "type": "number", "minimum": 0, "maximum": 1,
            "description": (
                "Pouze pro half_hip: 0=sedlová, 1=valbová. "
                "Délka valby = ratio × (width/2)."
            ),
        },
        "eave_overhang": {
            "type": "number", "minimum": 0,
            "description": "Přesah okapu od líce zdi v metrech (na všech stranách).",
        },
        "name": {"type": "string", "description": "Volitelný název projektu."},
    },
    "additionalProperties": False,
}


_FOOTPRINT_SCHEMA = {
    "type": ["object", "null"],
    "description": (
        "Půdorys jako polygon v metrech. null = budova zatím nenakreslená. "
        "Kreslený polygon JE obrys střechy (okapová hrana)."
    ),
    "required": ["vertices", "edges"],
    "properties": {
        "vertices": {
            "type": "array", "minItems": 3,
            "items": {
                "type": "array", "minItems": 2, "maxItems": 2,
                "items": {"type": "number"},
            },
            "description": (
                "Vrcholy [x, y] v metrech, jednoduchý (neprotínající se) "
                "polygon. Pořadí po nebo proti směru hodinových ručiček — "
                "normalizuje se automaticky."
            ),
        },
        "edges": {
            "type": "array",
            "items": {
                "type": "object",
                "required": ["type"],
                "properties": {
                    "type": {
                        "type": "string", "enum": ["eave", "gable"],
                        "description": (
                            "eave=okap, gable=štít. Hrana i vede z vrcholu i "
                            "do vrcholu i+1; počet hran = počet vrcholů. Štít "
                            "je platný jen na konci křídla s kolmými sousedy."
                        ),
                    },
                },
            },
        },
    },
}

_BUILDING_SCHEMA = {
    "type": "object",
    "description": "Jedno střešní těleso (budova) návrhu.",
    "properties": {
        "name": {"type": "string", "description": "Volitelný název stavby."},
        "footprint": _FOOTPRINT_SCHEMA,
        "pitch_deg": {
            "type": "number", "exclusiveMinimum": 0, "exclusiveMaximum": 90,
            "description": "Hlavní sklon střechy ve stupních.",
        },
        "overhang_m": {
            "type": "number", "minimum": 0,
            "description": "Přesah okapu (m) — stěny se v 3D odsadí dovnitř.",
        },
        "wall_height_m": {
            "type": "number", "exclusiveMinimum": 0,
            "description": "Výška stěn (m); okap leží ve výšce base+wall.",
        },
        "base_height_m": {
            "type": "number", "minimum": 0,
            "description": (
                "Výška budovy = elevace spodku stěn nad terénem (m). "
                "Umožňuje položit budovu výš (věž/patro nad nižší budovou)."
            ),
        },
        "half_hip_m": {
            "type": "number", "minimum": 0,
            "description": (
                "Polovalba: hloubka useknutí štítů OBDÉLNÍKU se dvěma štíty "
                "(m). 0 = bez polovalby."
            ),
        },
        "pitch_back_deg": {
            "type": ["number", "null"], "exclusiveMinimum": 0,
            "exclusiveMaximum": 90,
            "description": (
                "Asymetrická sedlová: sklon zadní roviny (obdélník se dvěma "
                "štíty). null = symetrická."
            ),
        },
    },
}

_PROJECT_JSON_SCHEMA = {
    "$schema": "http://json-schema.org/draft-07/schema#",
    "title": "Tectora project (schema 2.0) — polygonový návrh s budovami",
    "type": "object",
    "required": ["buildings"],
    "properties": {
        "schema_version": {"type": "string", "description": 'Pište "2.0".'},
        "project_name": {"type": "string"},
        "units": {"type": "string", "description": 'Vždy "m".'},
        "buildings": {
            "type": "array", "minItems": 1, "items": _BUILDING_SCHEMA,
            "description": "Návrh = jedna či více budov ve společné rovině x-y.",
        },
        "note": {
            "type": "string",
            "description": "Volitelná poznámka (jen v .tectora souborech).",
        },
        "location": {
            "type": ["object", "null"],
            "description": "Volitelná lokalita {name, lat, lon}.",
        },
    },
}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        prog="tectora",
        description="Tectora — kalkulátor střech.",
    )
    parser.add_argument("--version", action="version", version=f"Tectora {__version__}")
    sub = parser.add_subparsers(dest="cmd", required=True)

    # ---- calculate subcommand
    p_calc = sub.add_parser(
        "calculate",
        help="Spočítá střechu ze spec souboru (JSON/XLSX) nebo z parametrů.",
    )
    p_calc.add_argument("--spec", type=Path, help="Cesta ke spec souboru (.json nebo .xlsx)")
    p_calc.add_argument(
        "--project", type=Path,
        help="Cesta k projektu (.tectora/.json, schema 2.0) — spočítá VŠECHNY budovy.",
    )
    p_calc.add_argument(
        "-o", "--output", type=Path,
        help="Cesta pro výstup (.json nebo .xlsx). Bez tohoto se výsledek vypíše na stdout.",
    )
    # Inline parameters — alternative to --spec
    p_calc.add_argument("--type", choices=[t.value for t in RoofType])
    p_calc.add_argument("--length", type=float)
    p_calc.add_argument("--width", type=float)
    p_calc.add_argument("--pitch", type=float)
    p_calc.add_argument("--pitch-back", type=float)
    p_calc.add_argument("--half-hip-ratio", type=float, default=0.5)
    p_calc.add_argument("--eave-overhang", type=float, default=0.0)
    p_calc.add_argument("--name", default="")
    p_calc.add_argument("--quiet", "-q", action="store_true",
                        help="Při zápisu do souboru potlač shrnutí na stdout.")

    # ---- template subcommand
    p_tpl = sub.add_parser("template", help="Vygeneruje prázdnou Excel šablonu pro vstup.")
    p_tpl.add_argument("-o", "--output", type=Path, required=True)

    # ---- schema subcommand
    p_schema = sub.add_parser("schema", help="Vypíše JSON schéma vstupu (pro LLM).")
    p_schema.add_argument(
        "--project", action="store_true",
        help="Schéma polygonového projektu (schema 2.0) místo parametrické spec.",
    )

    # ---- validate subcommand
    p_val = sub.add_parser(
        "validate",
        help="Zkontroluje projektový soubor (.tectora/.json): polygony, štíty, spočitatelnost.",
    )
    p_val.add_argument("file", type=Path)
    p_val.add_argument("--json", action="store_true",
                       help="Strojový výstup {valid, problems, warnings}.")

    # ---- types subcommand
    sub.add_parser("types", help="Vypíše přehled podporovaných typů střech.")

    # ---- library
    p_lib = sub.add_parser("library", help="Knihovna zakázek (CRUD)")
    lib_sub = p_lib.add_subparsers(dest="lib_cmd", required=True)
    la = lib_sub.add_parser("add", help="Přidá projekt do knihovny")
    la.add_argument("project")
    la.add_argument("--db", default=None)
    la.add_argument("--name", required=True)
    la.add_argument("--note", default="")
    la.add_argument("--location", default=None)
    ll = lib_sub.add_parser("list", help="Vypíše zakázky (JSON)")
    ll.add_argument("--db", default=None)
    lg = lib_sub.add_parser("get", help="Uloží projekt zakázky do souboru")
    lg.add_argument("id", type=int)
    lg.add_argument("--db", default=None)
    lg.add_argument("--output", required=True)
    lu = lib_sub.add_parser("update", help="Upraví metadata zakázky")
    lu.add_argument("id", type=int)
    lu.add_argument("--db", default=None)
    lu.add_argument("--name", default=None)
    lu.add_argument("--note", default=None)
    lu.add_argument("--location", default=None)
    ld = lib_sub.add_parser("delete", help="Smaže zakázku")
    ld.add_argument("id", type=int)
    ld.add_argument("--db", default=None)

    # ---- geocode
    p_geo = sub.add_parser("geocode", help="Našeptá místa (JSON)")
    p_geo.add_argument("query")
    p_geo.add_argument("--limit", type=int, default=5)

    args = parser.parse_args(argv)

    if args.cmd == "calculate":
        return _cmd_calculate(args)
    if args.cmd == "template":
        write_template_xlsx(args.output)
        print(f"Šablona vytvořena: {args.output}", file=sys.stderr)
        return 0
    if args.cmd == "schema":
        schema = _PROJECT_JSON_SCHEMA if args.project else _JSON_SCHEMA
        json.dump(schema, sys.stdout, indent=2, ensure_ascii=False)
        sys.stdout.write("\n")
        return 0
    if args.cmd == "validate":
        return _cmd_validate(args)
    if args.cmd == "types":
        return _cmd_types()
    if args.cmd == "library":
        return _cmd_library(args)
    if args.cmd == "geocode":
        return _cmd_geocode(args)
    return 1


def _build_spec_from_args(args) -> RoofSpec:
    if args.type is None or args.length is None or args.width is None or args.pitch is None:
        raise SystemExit(
            "Pro inline výpočet jsou vyžadovány --type, --length, --width a --pitch. "
            "Případně použij --spec <soubor>."
        )
    return RoofSpec(
        type=RoofType(args.type),
        length=args.length,
        width=args.width,
        pitch=args.pitch,
        pitch_back=args.pitch_back,
        half_hip_ratio=args.half_hip_ratio,
        eave_overhang=args.eave_overhang,
        name=args.name,
    )


def _load_spec(path: Path) -> RoofSpec:
    if path.suffix.lower() == ".json":
        return read_spec_json(path)
    if path.suffix.lower() in (".xlsx", ".xlsm"):
        return read_spec_xlsx(path)
    raise SystemExit(f"Nepodporovaný formát spec souboru: {path.suffix}")


def _cmd_calculate(args) -> int:
    if args.project is not None:
        if args.spec is not None:
            print("Chyba: --project a --spec nelze kombinovat.", file=sys.stderr)
            return 2
        return _cmd_calculate_project(args)
    if args.spec is not None:
        spec = _load_spec(args.spec)
    else:
        spec = _build_spec_from_args(args)

    try:
        result = calculate(spec)
    except ValueError as exc:
        print(f"Chyba: {exc}", file=sys.stderr)
        return 2

    if args.output is not None:
        ext = args.output.suffix.lower()
        if ext == ".json":
            write_result_json(result, args.output)
        elif ext in (".xlsx", ".xlsm"):
            write_report_xlsx(result, args.output)
        else:
            print(f"Chyba: neznámý výstupní formát {ext}", file=sys.stderr)
            return 2
        if not args.quiet:
            _print_summary(result)
            print(f"\nUloženo do: {args.output}", file=sys.stderr)
    else:
        json.dump(result.as_dict(), sys.stdout, indent=2, ensure_ascii=False)
        sys.stdout.write("\n")
    return 0


def _building_label(building, index: int) -> str:
    return building.name or f"Stavba {index + 1}"


def _cmd_calculate_project(args) -> int:
    """Compute EVERY building of a schema-2.0 project file (like the GUI calc).

    Output: design JSON on stdout, or --output file.json / file.xlsx (the
    xlsx matches the GUI export: per-building sections, totals, heights above
    ground and the material estimate). Any failing building aborts with exit
    code 2 and a message naming it, so an AI caller can fix the input.
    """
    from tectora.core.materials import MaterialSpec
    from tectora.core.roof_generator import generate_roof
    from tectora.io.excel import write_design_report_xlsx
    from tectora.io.project_file import load_project_file

    try:
        project, _note, _location = load_project_file(str(args.project))
    except (OSError, ValueError, KeyError) as exc:
        print(f"Chyba: projekt nelze načíst: {exc}", file=sys.stderr)
        return 2

    results, names, eaves = [], [], []
    for i, b in enumerate(project.buildings):
        if b.footprint is None:
            continue
        label = _building_label(b, i)
        try:
            result = generate_roof(
                b.footprint, b.pitch_deg, name=b.name or project.project_name,
                overhang=b.overhang_m, half_hip_m=b.half_hip_m,
                pitch_back=b.pitch_back_deg,
            )
        except ValueError as exc:
            print(f"Chyba: {label}: {exc}", file=sys.stderr)
            return 2
        results.append(result)
        names.append(label)
        eaves.append(b.base_height_m + b.wall_height_m)

    if not results:
        print("Chyba: projekt nemá žádnou budovu s půdorysem.", file=sys.stderr)
        return 2

    design = uc.DesignResult(results)
    out = {
        "project_name": project.project_name,
        "buildings": [
            {"name": names[i], "eave_height_m": round(eaves[i], 3),
             **results[i].as_dict()}
            for i in range(len(results))
        ],
        "totals": {
            "total_area_m2": round(design.total_area_m2, 3),
            "ridge_length_m": round(design.total_ridge_length_m, 3),
            "hip_length_m": round(design.total_hip_length_m, 3),
            "valley_length_m": round(design.total_valley_length_m, 3),
            "eaves_perimeter_m": round(design.total_eaves_perimeter_m, 3),
            "gable_edge_total_m": round(design.total_gable_edge_m, 3),
        },
    }

    if args.output is not None:
        ext = args.output.suffix.lower()
        if ext == ".json":
            with open(args.output, "w", encoding="utf-8") as f:
                json.dump(out, f, indent=2, ensure_ascii=False)
        elif ext in (".xlsx", ".xlsm"):
            write_design_report_xlsx(design, args.output, building_names=names,
                                     building_eaves=eaves,
                                     material_spec=MaterialSpec())
        else:
            print(f"Chyba: neznámý výstupní formát {ext}", file=sys.stderr)
            return 2
        if not args.quiet:
            print(f"Uloženo do: {args.output}", file=sys.stderr)
    else:
        json.dump(out, sys.stdout, indent=2, ensure_ascii=False)
        sys.stdout.write("\n")
    return 0


def _validate_project(data: dict) -> tuple[list[str], list[str]]:
    """Return (problems, warnings) for a parsed project dict."""
    from tectora.core.gable import is_valid_gable_edge
    from tectora.core.geom2d import polygons_overlap
    from tectora.core.roof_generator import generate_roof

    problems: list[str] = []
    warnings: list[str] = []

    try:
        project = Project.from_dict(data)
    except (ValueError, KeyError, TypeError) as exc:
        return ([f"Projekt nelze načíst: {exc}"], warnings)

    drawn = []
    for i, b in enumerate(project.buildings):
        label = _building_label(b, i)
        if not (0 < b.pitch_deg < 90):
            problems.append(f"{label}: sklon musí být 1–89°, je {b.pitch_deg}.")
        if b.pitch_back_deg is not None and not (0 < b.pitch_back_deg < 90):
            problems.append(
                f"{label}: zadní sklon musí být 1–89° nebo null, je {b.pitch_back_deg}.")
        if b.wall_height_m <= 0:
            problems.append(f"{label}: výška stěn musí být kladná.")
        if b.overhang_m < 0 or b.base_height_m < 0 or b.half_hip_m < 0:
            problems.append(
                f"{label}: přesah, výška budovy ani polovalba nesmí být záporné.")
        if b.footprint is None:
            warnings.append(f"{label}: nemá půdorys (bude přeskočena).")
            continue
        drawn.append(b)

        for ei, e in enumerate(b.footprint.edges):
            if e.type.value == "gable" and not is_valid_gable_edge(b.footprint, ei):
                problems.append(
                    f"{label}: hrana {ei} nemůže být štít — štít je platný jen "
                    f"na konci křídla s kolmými sousedními hranami.")

        try:
            generate_roof(b.footprint, b.pitch_deg, overhang=b.overhang_m,
                          half_hip_m=b.half_hip_m, pitch_back=b.pitch_back_deg)
        except ValueError as exc:
            problems.append(f"{label}: výpočet selhal: {exc}")

    if not drawn:
        problems.append("Projekt nemá žádnou budovu s půdorysem.")

    for i in range(len(drawn)):
        for j in range(i + 1, len(drawn)):
            if polygons_overlap(list(drawn[i].footprint.vertices),
                                list(drawn[j].footprint.vertices)):
                warnings.append(
                    "Stavby se v půdorysu překrývají — překrytá plocha bude "
                    "v součtu dvakrát.")
                break
        else:
            continue
        break

    return (problems, warnings)


def _cmd_validate(args) -> int:
    try:
        with open(args.file, encoding="utf-8") as f:
            data = json.load(f)
    except (OSError, json.JSONDecodeError) as exc:
        problems, warnings = [f"Soubor nelze načíst jako JSON: {exc}"], []
    else:
        problems, warnings = _validate_project(data)

    valid = not problems
    if args.json:
        json.dump({"valid": valid, "problems": problems, "warnings": warnings},
                  sys.stdout, ensure_ascii=False)
        sys.stdout.write("\n")
    else:
        for p in problems:
            print(f"CHYBA: {p}")
        for w in warnings:
            print(f"Upozornění: {w}")
        print("OK — projekt je platný." if valid else "Projekt NENÍ platný.")
    return 0 if valid else 2


def _print_summary(result: RoofResult) -> None:
    """Brief human-readable summary (written to stderr to avoid polluting JSON pipes)."""
    s = result.spec
    print(file=sys.stderr)
    print(f"  Typ:               {s.type.cs_name} ({s.type.value})", file=sys.stderr)
    print(f"  Půdorys:           {s.length} × {s.width} m", file=sys.stderr)
    print(f"  Sklon:             {s.pitch}°", file=sys.stderr)
    print(file=sys.stderr)
    print(f"  Plocha:            {result.total_area_m2:8.2f} m²", file=sys.stderr)
    print(f"  Hřeben:            {result.ridge_length_m:8.2f} m", file=sys.stderr)
    print(f"  Nároží ({result.hip_count}):       {result.hip_length_m:8.2f} m", file=sys.stderr)
    print(f"  Úžlabí:            {result.valley_length_m:8.2f} m", file=sys.stderr)
    print(f"  Krajové tašky:     {result.gable_edge_total_m:8.2f} m  "
          f"(L={result.gable_edge_left_m:.2f} / P={result.gable_edge_right_m:.2f})",
          file=sys.stderr)
    print(f"  Okap:              {result.eaves_perimeter_m:8.2f} m", file=sys.stderr)
    print(f"  Výška hřebene:     {result.apex_height_m:8.2f} m", file=sys.stderr)


def _cmd_types() -> int:
    print("Podporované typy střech:")
    print()
    descriptions = {
        RoofType.SHED: "1 sklonová rovina; vyšší okraj na zadní straně",
        RoofType.GABLE: "2 sklonové roviny + 2 štíty; asym. možný přes pitch_back",
        RoofType.HIP: "4 sklony do hřebene; bez štítů",
        RoofType.HALF_HIP: "Sedlová s useknutými konci (strmější valbičky)",
        RoofType.PYRAMID: "4 sklony do bodu; vyžaduje čtvercový půdorys",
    }
    for t in RoofType:
        print(f"  {t.value:12s} {t.cs_name:14s} {descriptions[t]}")
    return 0


def _cmd_library(args) -> int:
    from tectora.io.sqlite_library import SqliteProjectLibrary, default_db_path

    library = SqliteProjectLibrary(args.db or default_db_path())
    try:
        if args.lib_cmd == "add":
            with open(args.project, encoding="utf-8") as f:
                project = Project.from_dict(json.load(f))
            try:
                result = uc.calculate_project(project).as_dict()
            except ValueError:
                result = None  # e.g. non-rectangle footprint: store without cached result
            location = Location(args.location) if args.location else None
            eid = uc.save_entry(library, project, args.name,
                                note=args.note, location=location, result=result)
            json.dump({"id": eid}, sys.stdout, ensure_ascii=False)
            sys.stdout.write("\n")
            return 0
        if args.lib_cmd == "list":
            rows = [entry_summary(e) for e in uc.list_entries(library)]
            json.dump(rows, sys.stdout, ensure_ascii=False)
            sys.stdout.write("\n")
            return 0
        if args.lib_cmd == "get":
            entry = uc.load_entry(library, args.id)
            with open(args.output, "w", encoding="utf-8") as f:
                json.dump(entry.project.to_dict(), f, ensure_ascii=False, indent=2)
            return 0
        if args.lib_cmd == "update":
            changes = {"name": args.name, "note": args.note}
            if args.location is not None:
                changes["location"] = Location(args.location)
            uc.update_entry(library, args.id, **changes)
            return 0
        if args.lib_cmd == "delete":
            uc.delete_entry(library, args.id)
            return 0
    except (EntryNotFound, ValueError) as exc:
        print(f"Chyba: {exc}", file=sys.stderr)
        return 2
    return 1


def _cmd_geocode(args) -> int:
    from tectora.io.nominatim_geocoder import NominatimGeocoder

    places = NominatimGeocoder().search(args.query, args.limit)
    json.dump([{"name": p.name, "lat": p.lat, "lon": p.lon} for p in places],
              sys.stdout, ensure_ascii=False)
    sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
