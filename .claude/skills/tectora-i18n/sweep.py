#!/usr/bin/env python3
"""Localization sweep for the Tectora UI. Standard library only.

Checks what users can see for the project rules: four complete languages,
no DACAD name, no DACAD error codes and no German roofing words outside the
German translation.

    python3 .claude/skills/tectora-i18n/sweep.py             # from the repo root
    python3 .claude/skills/tectora-i18n/sweep.py --json

Checks:
  keys          every key in cs exists in sk, en and de, and no extra keys
  placeholders  {name} placeholders are the same in every language
  empty         no empty values
  dacad         no "DACAD" or "COIN" in any value
  codes         no raw core error codes (E00002, 0xa0f ...) in any value
  german        no German roofing words in cs, sk or en values
  templates     no "DACAD" in Svelte markup or string literals (comments and
                identifiers such as DACAD_MATERIALS are fine)

Exit code 1 when anything is found.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

I18N = Path("app/src/lib/i18n.svelte.ts")
UI_DIR = Path("app/src")
LOCALES = ("cs", "sk", "en", "de")
# DACAD's German terms and messages that must not leak into other languages.
GERMAN = [
    "Traufe", "Traufhöhe", "Ortgang", "Giebel", "Kehle", "Krüppelwalm", "Gaube",
    "Gauben", "Dachfläche", "Dachflächen", "Schornstein", "Überstand", "Vorsprung",
    "Nicht", "Fläche", "Flächen", "Berechnung", "Dachlinie", "Firsthöhe", "Pultdach",
    "Walmdach", "Satteldach", "Ziegel", "Blech", "Schiefer", "Hilfslinie",
]
GERMAN_RE = re.compile(r"\b(" + "|".join(map(re.escape, GERMAN)) + r")\b")
BRAND_RE = re.compile(r"\b(DACAD|COIN)\b", re.I)
CODE_RE = re.compile(r"\bE0\d{3,4}\b|\b0x[0-9a-f]{3}\b", re.I)
PLACEHOLDER_RE = re.compile(r"\{(\w+)\}")
ENTRY_RE = re.compile(r'^\s*"([^"]+)"\s*:\s*(".*")\s*,?\s*$')


def parse_dicts(text: str) -> dict[str, dict[str, str]]:
    """The `const cs: Dict = { "key": "value", ... };` blocks."""
    out: dict[str, dict[str, str]] = {}
    current: str | None = None
    for line in text.splitlines():
        if m := re.match(r"^const (\w+): Dict = \{", line):
            current = m.group(1)
            out[current] = {}
            continue
        if current and line.startswith("};"):
            current = None
            continue
        if current and (m := ENTRY_RE.match(line)):
            out[current][m.group(1)] = json.loads(m.group(2))
    return out


def strip_comments(src: str) -> str:
    src = re.sub(r"<!--.*?-->", " ", src, flags=re.S)
    src = re.sub(r"/\*.*?\*/", " ", src, flags=re.S)
    return re.sub(r"(^|[^:\"'])//[^\n]*", r"\1", src)


def sweep(root: Path) -> list[dict[str, str]]:
    found: list[dict[str, str]] = []

    def hit(check: str, where: str, detail: str) -> None:
        found.append({"check": check, "where": where, "detail": detail})

    dicts = parse_dicts((root / I18N).read_text(encoding="utf-8"))
    missing = [loc for loc in LOCALES if loc not in dicts]
    if missing:
        hit("keys", str(I18N), f"no dictionary for {', '.join(missing)}")
    base = dicts.get("cs", {})
    for loc in LOCALES:
        d = dicts.get(loc, {})
        for key in sorted(set(base) - set(d)):
            hit("keys", f"{loc}:{key}", "missing")
        for key in sorted(set(d) - set(base)):
            hit("keys", f"{loc}:{key}", "not in cs")
        for key, value in d.items():
            where = f"{loc}:{key}"
            if not value.strip():
                hit("empty", where, "empty value")
            if key in base and set(PLACEHOLDER_RE.findall(value)) != set(PLACEHOLDER_RE.findall(base[key])):
                hit("placeholders", where, f"{value!r} vs cs {base[key]!r}")
            if m := BRAND_RE.search(value):
                hit("dacad", where, f"{m.group(0)} in {value!r}")
            if m := CODE_RE.search(value):
                hit("codes", where, f"{m.group(0)} in {value!r}")
            if loc != "de" and (m := GERMAN_RE.search(value)):
                hit("german", where, f"{m.group(0)} in {value!r}")

    for path in sorted((root / UI_DIR).rglob("*")):
        if path.suffix not in {".svelte", ".ts"} or path.name.endswith(".test.ts") or path.name == I18N.name:
            continue
        if "dev" in path.relative_to(root / UI_DIR).parts:
            continue
        text = strip_comments(path.read_text(encoding="utf-8"))
        for n, line in enumerate(text.splitlines(), 1):
            for m in re.finditer(r"\bDACAD\b(?!_)", line):
                rel = path.relative_to(root)
                hit("templates", f"{rel}:{n}", line.strip()[:100])
                break
    return found


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("root", nargs="?", default=".", help="repository root")
    ap.add_argument("--json", action="store_true")
    a = ap.parse_args()
    found = sweep(Path(a.root))
    if a.json:
        print(json.dumps(found, indent=2, ensure_ascii=False))
    else:
        for f in found:
            print(f"{f['check']:<13} {f['where']}: {f['detail']}")
        print(f"{len(found)} finding(s)" if found else "clean")
    return 1 if found else 0


if __name__ == "__main__":
    sys.exit(main())
