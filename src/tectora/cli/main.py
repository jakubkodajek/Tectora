"""CLI interface for Tectora.

Usage examples:

    tectora calculate --type gable --length 10 --width 6 --pitch 30
    tectora calculate --spec roof.json --output result.json
    tectora calculate --spec input.xlsx --output report.xlsx
    tectora template --output input.xlsx
    tectora schema                       # JSON schema for LLMs
    tectora types                        # list of supported roof types

For AI: use `tectora schema` to obtain the input structure.
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
from tectora.core import RoofSpec, RoofType, calculate
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
    sub.add_parser("schema", help="Vypíše JSON schéma vstupu (pro LLM).")

    # ---- types subcommand
    sub.add_parser("types", help="Vypíše přehled podporovaných typů střech.")

    args = parser.parse_args(argv)

    if args.cmd == "calculate":
        return _cmd_calculate(args)
    if args.cmd == "template":
        write_template_xlsx(args.output)
        print(f"Šablona vytvořena: {args.output}", file=sys.stderr)
        return 0
    if args.cmd == "schema":
        json.dump(_JSON_SCHEMA, sys.stdout, indent=2, ensure_ascii=False)
        sys.stdout.write("\n")
        return 0
    if args.cmd == "types":
        return _cmd_types()
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


if __name__ == "__main__":
    raise SystemExit(main())
