# Reference CLI nástrojů

Všechny příkazy: `tectora <příkaz> …`. Úspěch = exit 0; chyba = exit 2 +
česká zpráva na stderr. Výstupy určené ke strojovému čtení jdou na stdout
(JSON), lidská shrnutí na stderr.

## `tectora schema [--project]`

Vypíše JSON schéma vstupu.

- bez přepínače: **parametrická spec** (obdélník + typ střechy) pro rychlé
  jednoduché výpočty přes `calculate --spec` / inline parametry,
- `--project`: **schéma projektu 2.0** (polygonové půdorysy, více budov) —
  tohle je formát GUI, knihovny i `.tectora` souborů. Každé pole má český
  popis. **Vždy začni odsud.**

## `tectora calculate`

Tři režimy (vzájemně výlučné):

1. **Inline parametry** — `--type gable --length 10 --width 6 --pitch 30`
   (+ volitelně `--pitch-back`, `--half-hip-ratio`, `--eave-overhang`,
   `--name`). Jen obdélníky.
2. **`--spec soubor.json|.xlsx`** — táž parametrická spec ze souboru.
3. **`--project soubor.tectora|.json`** — spočítá **všechny budovy**
   polygonového projektu (stejný výpočet jako tlačítko „Provést výpočet"
   v GUI, včetně polovalby a zadního sklonu).

Výstup:
- bez `--output`: JSON na stdout. U `--project` má tvar
  `{project_name, buildings: [{name, eave_height_m, input, results, faces,
  …}], totals: {…}}`.
- `--output soubor.json`: týž JSON do souboru.
- `--output soubor.xlsx`: formátovaný výkaz výměr. U `--project` odpovídá
  exportu z GUI: sekce po budovách (včetně výšky okapu/hřebene nad
  terénem), „Celkem za návrh" a „Materiál (odhad)".
- `-q/--quiet`: potlačí shrnutí na stderr při zápisu do souboru.

Selže-li kterákoli budova (např. neplatný štít), příkaz skončí exit 2 a
vypíše, KTERÁ budova a proč.

## `tectora validate <soubor> [--json]`

Zkontroluje projektový soubor před použitím:

- načtitelnost JSON + schéma (polygony se nesmí protínat, ≥ 3 vrcholy,
  počet hran = počet vrcholů),
- rozsahy parametrů (sklon 1–89°, kladná výška stěn, nezáporný přesah /
  výška budovy / polovalba),
- platnost štítů po hranách (s indexem vadné hrany),
- spočitatelnost každé budovy (zkusmý výpočet),
- překryvy půdorysů (jen **upozornění**, ne chyba).

`--json` → `{"valid": bool, "problems": [...], "warnings": [...]}` na
stdout. Exit 0 = platný (i s upozorněními), exit 2 = neplatný.

## `tectora library …` — knihovna zakázek (sdílená s GUI)

Všechny podpříkazy mají volitelné `--db <cesta>` (výchozí
`%APPDATA%/Tectora/tectora.db`).

| Příkaz | Popis |
|---|---|
| `library add <projekt.json> --name "Novák – RD" [--note …] [--location …]` | Uloží projekt jako zakázku; vypíše `{"id": N}`. |
| `library list` | JSON seznam zakázek (id, name, note, location, časy). |
| `library get <id> --output projekt.json` | Uloží projekt zakázky do souboru. |
| `library update <id> [--name …] [--note …] [--location …]` | Upraví metadata. |
| `library delete <id>` | Smaže zakázku. |

Pozn.: `library add` počítá do karty zakázky orientační výsledek první
budovy; plný výpočet dělej přes `calculate --project`.

## `tectora geocode "<dotaz>" [--limit N]`

Našeptávač míst (OpenStreetMap/Nominatim, bez API klíče). Vrací JSON
`[{name, lat, lon}, …]`. Vyžaduje internet; offline vrací `[]`.

## `tectora template --output vstup.xlsx`

Vygeneruje prázdnou Excel šablonu parametrické spec (pro ruční vyplnění
člověkem; agent ji zpravidla nepotřebuje).

## `tectora types`

Přehled parametrických typů střech (shed/gable/hip/half_hip/pyramid)
s českými názvy — hodí se pro komunikaci s uživatelem.

## Rychlý příklad (celý cyklus)

```bash
tectora schema --project > schema.json      # 1. zjisti formát
# 2. sestav projekt.json podle podkladů (viz skill-projekt-z-podkladu.md)
tectora validate projekt.json               # 3. opravuj, dokud není exit 0
tectora calculate --project projekt.json    # 4. zkontroluj čísla (JSON)
tectora library add projekt.json --name "Novák – RD" --location "Brno"
tectora calculate --project projekt.json -o vykaz.xlsx -q   # 5. výkaz
```
