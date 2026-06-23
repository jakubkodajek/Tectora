# Tectora — Roadmap

> Princip: **TDD** (testy napřed), inkrementální dodávky, **hexagonální
> architektura** (Ports & Adapters) — viz `docs/specifikace.md` §7.
> Stavíme **zevnitř ven**: `domain` → `application` (+ porty) → `adapters` → `gui`.
> Vše je od začátku **CLI-ready**, aby na to ve Fázi 2 mohl navázat AI agent.

Legenda: `[ ]` plánováno · `[~]` rozpracováno · `[x]` hotovo

---

## Fáze 1 — Desktop aplikace (CLI-ready)

Pořadí: **doména → aplikace + CLI → knihovna → GUI (úvodní stránka + 2D) → Excel → 3D → i18n → živá sync.**

### 1.1 Doména (`tectora.domain`) — TDD
- [ ] Datový model projektu + validace polygonu (uzavřenost, sebeprůnik, ≥ 3 body)
- [ ] Straight-skeleton z polygonu (obdélník, L, T, U)
- [ ] Zvednutí osnovy do 3D dle globálního sklonu
- [ ] Klasifikace hran skeletu: hřeben / nároží / úžlabí
- [ ] Generování střešních rovin
- [ ] Výpočty: plocha (celkem i po rovinách), hřeben, nároží (délka+počet),
      úžlabí (délka+počet), obvod okapu, krajové hrany levá/pravá
- [ ] `Location` value object (název místa, lat, lon)
- [ ] Property-based testy (invarianty) + golden-file testy (sedlová/valbová/stanová/L/pultová)

### 1.2 Aplikační vrstva + adaptéry I/O (`tectora.application`, `tectora.adapters`)
- [ ] Use-casy: CreateProject, SetFootprint, SetEdgeType, SetPitch, CalculateRoof, ExportReport
- [ ] Porty (Protocol): `ProjectRepository`, `ReportExporter`
- [ ] Adaptér `json_repository`: načtení/uložení projektu (JSON, schéma v1.0)
- [ ] Serializace výsledků do JSON (DTO)
- [ ] Adaptér `excel_exporter` (openpyxl): výkaz — totály + rozpis po rovinách
- [ ] Use-casy testované s fake porty; adaptéry integračně

### 1.3 CLI adaptér (`tectora.cli`)
- [ ] `new`, `set-footprint`, `set-edge`, `set-pitch`
- [ ] `calculate` (→ JSON), `export` (→ xlsx), `show` (čitelné shrnutí)
- [ ] Strojový výstup + chybové kódy (`{ "error": ... }`, nenulový exit)
- [ ] Testy přes subprocess + golden JSON

### 1.4 Knihovna zakázek (SQLite) + poloha — backend + CLI
- [ ] `LibraryEntry` (projekt + název + poznámka + poloha + výsledek + časy)
- [ ] Porty (Protocol): `ProjectLibrary` (CRUD), `GeocodingService` (search)
- [ ] Adaptér `sqlite_library`: SQLite v app-data (`%APPDATA%/Tectora/tectora.db`)
- [ ] Adaptér `nominatim_geocoder`: našeptávač míst (OSM/Nominatim), timeout, offline fallback
- [ ] Use-casy: SaveEntry, LoadEntry, ListEntries, UpdateEntry, DeleteEntry, SuggestLocation
- [ ] CLI: `library list/add/get/update/delete`, `geocode "<dotaz>"`
- [ ] Testy: SQLite v dočasné DB; geocoder s fake HTTP

### 1.5 GUI adaptér — úvodní stránka + 2D editor (`tectora.gui`)
- [ ] **Úvodní stránka**: tlačítko „Nové" + kolekce karet zakázek z knihovny
- [ ] Karta zakázky (náhled, název, poloha·datum, plocha) + menu „tři tečky" (CRUD)
- [ ] Hledání/filtrování zakázek, prázdný stav
- [ ] **Editor**: menu, kompaktní toolbar, pravý panel (Zakázka/Parametry/Výsledky), status bar
- [ ] Kreslení/úprava polygonu, mřížka + snap, číselné zadání rozměrů
- [ ] Přepínání typu hrany (okap/štít) kliknutím
- [ ] Pole Poloha s našeptávačem, pole Poznámka, vstup sklonu, tlačítko **Provést výpočet**
- [ ] Panel výsledků s jednotkami + vykreslení osnovy s legendou; **Uložit** do knihovny
- [ ] Smoke testy GUI (logika je v jádře)

### 1.6 Excel export z GUI
- [ ] Tlačítko „Export do Excelu" napojené na `excel_exporter`

### 1.7 3D náhled
- [ ] PyVista/pyvistaqt 3D model střechy (otáčení, zoom)
- [ ] Barevné rozlišení rovin a hran, přepínač 2D/3D

### 1.8 Lokalizace (cs/sk/en)
- [ ] Překladový katalog, přepínač jazyka, žádné natvrdo zadané řetězce

### 1.9 Živá synchronizace GUI ↔ CLI
- [ ] File-watcher (QFileSystemWatcher) → auto reload + přepočet + překreslení
- [ ] Nová/změněná zakázka od CLI/Claude se objeví na úvodní stránce
- [ ] Ošetření konfliktů a neuložených změn

**Hotovo Fáze 1 = ✅** funkční desktop kalkulátor s úvodní stránkou a knihovnou
zakázek, 2D/3D, polohou, Excel exportem, 3 jazyky a CLI, které umí vše co GUI.

---

## Fáze 2 — AI CLI režim (řízeno Claude Code)

- [ ] „Skills"/instrukce pro Claude Code (`docs/ai/`): popis CLI + JSON schématu + knihovny
- [ ] Doladění ergonomie CLI pro agenta (čisté chyby, idempotence, příklady)
- [ ] Workflow: uživatel pošle PDF do Claude → Claude přečte PDF → poskládá projekt
      přes CLI, uloží do knihovny → otevřené GUI se živě aktualizuje
- [ ] Příklady a end-to-end test celého toku
- [ ] (Volitelně) MCP server jako alternativa k CLI

**Pozn.:** Tectora sama **nevolá žádné LLM API** — AI běží vně a řídí nástroj.

---

## Fáze 3 — Rozšíření (budoucí)

- [ ] Různé sklony na jednotlivé hrany (weighted straight skeleton)
- [ ] Přesahy okapů, vikýře, světlíky, komíny
- [ ] Materiálové počty (tašky, latě, fólie)
- [ ] PDF report (výstupní)
- [ ] Tmavé téma
