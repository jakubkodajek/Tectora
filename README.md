# Tectora — Kalkulátor střech

Desktopová aplikace + CLI pro výpočet rozměrů střech. Všechny jednotky v metrech / m² / stupních.

## Co počítá

Pro zadaný obdélníkový/čtvercový půdorys a typ střechy:

- **Plocha střechy** (m²) — celková i po jednotlivých rovinách
- **Délka hřebene** (m)
- **Délka nároží** (m) a **počet nároží**
- **Délka úžlabí** (m) — pro Fázi 1 vždy 0 (úžlabí vznikají u L/T tvarů)
- **Délka krajových tašek** (štít) — levý / pravý
- **Obvod okapové hrany** (m)

## Podporované typy střech (Fáze 1)

| Typ | Český název | Popis |
|---|---|---|
| `shed` | pultová | 1 sklonová rovina |
| `gable` | sedlová | 2 sklonové roviny + 2 štíty |
| `hip` | valbová | 4 sklonové roviny meeting at ridge |
| `half_hip` | polovalbová | sedlová s useknutými štíty |
| `pyramid` | stanová | 4 sklonové roviny do bodu (čtvercový půdorys) |

## Instalace

```bash
pip install -e ".[gui,dev]"
```

### Windows exe (pro uživatele bez Pythonu)

```bash
pip install -e ".[gui,build]"
python packaging/build_exe.py     # → dist/Tectora.exe (jeden soubor, s ikonou)
```

Release na GitHubu se sestaví automaticky: push tagu `v*` spustí workflow
`release.yml`, které exe postaví, ověří (`--smoke`) a přiloží k release.

## Použití — GUI

```bash
tectora-gui
```

## Použití — CLI

```bash
# Z JSON specifikace
tectora calculate --spec roof.json --output result.json

# Z Excel šablony
tectora calculate --spec input.xlsx --output result.xlsx

# Inline rychlý výpočet
tectora calculate --type gable --length 12 --width 8 --pitch 35
```

## Fáze projektu

- **Fáze 1** (aktuální): desktopová aplikace + CLI pro obdélníkový půdorys
- **Fáze 2**: CLI režim pro AI — pošleš LLM (Claude/Gemini) technickou zprávu střechy, on vygeneruje JSON spec a Tectora to spočítá
- **Fáze 3**: obecné polygonální půdorysy (L, T, U tvary) — straight skeleton algoritmus
