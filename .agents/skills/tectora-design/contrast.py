#!/usr/bin/env python3
"""WCAG contrast check for the Tectora theme tokens. Standard library only.

Reads the CSS custom properties in app/src/app.css for every theme it finds
(the light `:root` block, the dark `prefers-color-scheme` block as
"dark (system)" and any `[data-theme=...]` block, e.g. "dark" as picked in
Settings) and checks the text / background pairs below. The two dark
blocks are checked separately, so a token missing from one of them shows.

    python3 .claude/skills/tectora-design/contrast.py            # app/src/app.css
    python3 .claude/skills/tectora-design/contrast.py path.css --json
    python3 .claude/skills/tectora-design/contrast.py "#5b6377" "#ffffff"

Thresholds (WCAG 2.2): normal text 4.5:1, large text and UI parts
(lines in the plan, borders, focus rings) 3:1.
Exit code 1 when any pair fails, so it can gate a commit.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

# (foreground, background, kind). kind: "text" 4.5, "large" 3.0, "ui" 3.0.
# "a|b" uses token a when the theme defines it, else b. Pairs whose tokens a
# theme does not define are skipped.
PAIRS: list[tuple[str, str, str]] = [
    ("text", "bg", "text"),
    ("text", "surface", "text"),
    ("text", "surface-2", "text"),
    ("text-2", "bg", "text"),
    ("text-2", "surface", "text"),
    ("text-2", "surface-2", "text"),
    ("text-3", "surface", "large"),
    ("accent", "surface", "text"),
    ("accent", "accent-soft", "text"),
    ("#ffffff", "accent-fill|accent", "text"),
    ("on-ai", "ai", "text"),
    ("#ffffff", "danger-fill", "text"),
    ("ai-strong", "ai-soft", "text"),
    ("ai-strong", "surface", "text"),
    ("danger", "surface", "text"),
    ("status-offer", "status-offer-bg", "text"),
    ("status-ordered", "status-ordered-bg", "text"),
    ("status-done", "status-done-bg", "text"),
    ("code-text", "code-bg", "text"),
    ("ok", "surface", "ui"),
    ("focus", "surface", "ui"),
    ("line-eave", "surface", "ui"),
    ("line-verge", "surface", "ui"),
    ("line-ridge", "surface", "ui"),
    ("line-hip", "surface", "ui"),
    ("line-valley", "surface", "ui"),
    ("line-pult", "surface", "ui"),
    ("line-knick", "surface", "ui"),
    ("line-gorge", "surface", "ui"),
    ("line-abutment", "surface", "ui"),
]
NEEDS = {"text": 4.5, "large": 3.0, "ui": 3.0}
HEX = re.compile(r"^#([0-9a-fA-F]{3}|[0-9a-fA-F]{6})$")
VAR = re.compile(r"--([a-z0-9-]+)\s*:\s*([^;]+);")


def parse_hex(value: str) -> tuple[int, int, int]:
    m = HEX.match(value.strip())
    if not m:
        raise ValueError(f"not a hex colour: {value!r}")
    h = m.group(1)
    if len(h) == 3:
        h = "".join(c * 2 for c in h)
    return int(h[0:2], 16), int(h[2:4], 16), int(h[4:6], 16)


def luminance(rgb: tuple[int, int, int]) -> float:
    def channel(c: int) -> float:
        s = c / 255
        return s / 12.92 if s <= 0.04045 else ((s + 0.055) / 1.055) ** 2.4

    r, g, b = (channel(c) for c in rgb)
    return 0.2126 * r + 0.7152 * g + 0.0722 * b


def ratio(fg: str, bg: str) -> float:
    a, b = luminance(parse_hex(fg)), luminance(parse_hex(bg))
    hi, lo = max(a, b), min(a, b)
    return (hi + 0.05) / (lo + 0.05)


def blocks(css: str) -> list[tuple[str, str]]:
    """(selector context, body) for every innermost `{...}` block."""
    out, stack, start = [], [], 0
    for i, ch in enumerate(css):
        if ch == "{":
            stack.append((css[start:i].strip().split("}")[-1].strip(), i))
            start = i + 1
        elif ch == "}" and stack:
            head, open_at = stack.pop()
            context = " ".join(h for h, _ in stack) + " " + head
            out.append((context.strip(), css[open_at + 1 : i]))
            start = i + 1
    return out


def themes(css: str) -> dict[str, dict[str, str]]:
    """Token values per theme. Dark and data-theme blocks inherit light values."""
    css = re.sub(r"/\*.*?\*/", "", css, flags=re.S)
    light: dict[str, str] = {}
    extra: dict[str, dict[str, str]] = {}
    for context, body in blocks(css):
        values = {k: v.strip() for k, v in VAR.findall(body)}
        if not values:
            continue
        if "prefers-color-scheme: dark" in context:
            extra.setdefault("dark (system)", {}).update(values)
        elif m := re.search(r"data-theme\s*=\s*['\"]?(\w+)", context):
            extra.setdefault(m.group(1), {}).update(values)
        elif context.endswith(":root"):
            light.update(values)
    result = {"light": light}
    for name, values in extra.items():
        merged = dict(light)
        merged.update(values)
        result[name] = merged
    return result


def resolve(name: str, tokens: dict[str, str]) -> tuple[str, str | None]:
    """The token used for `name` ("a|b" = first one defined) and its value."""
    if name.startswith("#"):
        return name, name
    for option in name.split("|"):
        if option in tokens:
            return option, tokens[option]
    return name, None


def check(tokens: dict[str, str]) -> list[dict[str, object]]:
    rows = []
    for fg, bg, kind in PAIRS:
        fg, fv = resolve(fg, tokens)
        bg, bv = resolve(bg, tokens)
        if not fv or not bv or not HEX.match(fv) or not HEX.match(bv):
            continue  # token missing or not a plain hex value
        r = ratio(fv, bv)
        rows.append({"fg": fg, "bg": bg, "kind": kind, "ratio": round(r, 2), "ok": r >= NEEDS[kind]})
    return rows


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("args", nargs="*", help="CSS file, or two hex colours")
    ap.add_argument("--json", action="store_true")
    a = ap.parse_args()

    if len(a.args) == 2 and all(HEX.match(x) for x in a.args):
        r = ratio(*a.args)
        print(f"{a.args[0]} on {a.args[1]}: {r:.2f}:1  text {'ok' if r >= 4.5 else 'FAIL'}  ui {'ok' if r >= 3 else 'FAIL'}")
        return 0 if r >= 4.5 else 1

    path = Path(a.args[0] if a.args else "app/src/app.css")
    report = {name: check(tokens) for name, tokens in themes(path.read_text(encoding="utf-8")).items()}
    failed = sum(not row["ok"] for rows in report.values() for row in rows)
    if a.json:
        print(json.dumps({"file": str(path), "themes": report, "failed": failed}, indent=2))
    else:
        for name, rows in report.items():
            print(f"[{name}]")
            for row in rows:
                mark = "ok  " if row["ok"] else "FAIL"
                print(f"  {mark} {row['fg']:>14} on {row['bg']:<12} {row['ratio']:5.2f}:1  ({row['kind']}, needs {NEEDS[row['kind']]})")
        print(f"{failed} failing pair(s)" if failed else "all pairs pass")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
