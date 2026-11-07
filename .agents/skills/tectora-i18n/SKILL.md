---
name: tectora-i18n
description: >-
  Adds and checks Tectora UI texts in Czech, Slovak, English and German with
  the project glossary, and sweeps the UI for anything that must not be seen:
  the DACAD name, raw core error codes, German DACAD terms in other languages,
  missing keys. Use when the user says "translate", "add a label", "new
  message", "wrong word", "it shows German", "it says DACAD", "check
  translations", or whenever a change adds user-visible text.
---

# tectora-i18n

Texts live in `app/src/lib/i18n.svelte.ts`: one dictionary per language
(`cs` is the source, then `sk`, `en`, `de`), used as `t("area.key", { n })`.

## The rules

- **Four languages, every time.** A new key goes into cs, sk, en and de in the
  same commit. No fallback to Czech or to the key on purpose.
- **No DACAD on screen.** Not the name, not COIN, not DACAD's German menu
  names, not raw codes like `E00002` or `Nicht geschlossene Fläche(n)`. Core
  errors are mapped to keys with a plain description of what to fix.
  DACAD terms are fine in code comments and docs.
- **Glossary.** A saved roof is a job: zakázka (cs), zákazka (sk), job (en),
  Auftrag (de), never návrh / design / Entwurf.
  - de: Traufe, Traufhöhe, Überstand (not Vorsprung), Giebel (edge type;
    Ortgang only for verge lines), Schornstein (not Kamin), Grat, Kehle,
    waagerechte Kehle, Pultabschluss, Knick.
  - cs / sk: okap / odkvap, štít, štítová hrana, pultová hrana, nároží /
    nárožie, úžlabí / úžľabie, vodorovné úžlabí, zlom; sk "Štandardná strecha".
  - en is British English (neighbour, anticlockwise); shed edge = "shed top edge".
- **Counts** as "Label: n", so Czech and Slovak plurals cannot go wrong.
- Units and numbers through `numberLocale()` (cs-CZ, sk-SK, en-GB, de-DE):
  decimal comma in cs, sk and de.
- Text answers from the core or MCP that reach the UI are translated in the UI,
  not in Rust.

## The sweep

```bash
python3 .claude/skills/tectora-i18n/sweep.py
```

It checks: missing or extra keys per language, different `{placeholders}`,
empty values, "DACAD" or "COIN" in any value, raw error codes, German roofing
words in cs / sk / en, and "DACAD" in Svelte markup or string literals outside
comments. Exit 1 on any finding. Where `app/src/lib/i18n.test.ts` exists it
guards the dictionary rules in CI; the sweep also covers the components.

## Adding text

1. Pick the key by screen and purpose (`editor.…`, `edge.…`, `calc.…`).
2. Write cs first, then sk, en, de with the glossary. Short: buttons are one or
   two words, hints one sentence.
3. Run the sweep and `cd app && npm test`.
4. Shoot the screen in de and cs at a narrow size (`tectora-ui-preview`):
   German is the longest and breaks layouts first.

## Output

Keys in all four languages, a clean sweep, passing tests, and a de screenshot
of the changed screen.
