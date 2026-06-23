# Tectora — Specifikace požadavků

> Verze 1.2 · 2026-06-23 · stav: **návrh k odsouhlasení**
> (v1.2: přidána knihovna zakázek v SQLite, úvodní stránka a poloha s našeptávačem)
> Kalkulátor střech — desktopová aplikace (GUI) + CLI ovladatelné AI.

---

## 1. Účel a vize

Tectora počítá rozměry střech z půdorysu nakresleného na plátně. Uživatel nakreslí
obrys budovy jako polygon, označí, které hrany jsou **okapy** a které **štíty**,
nastaví **sklon**, a aplikace automaticky vygeneruje geometrii střechy (přes
straight-skeleton algoritmus) a spočítá výměry.

Aplikace má dvě tváře nad **jedním společným jádrem**:

1. **GUI** — vizuální kreslení, 3D náhled, výsledky, export do Excelu.
2. **CLI** — stejné výpočty ovladatelné z příkazové řádky; navrženo tak, aby ho
   uměl řídit AI agent (Claude Code) pomocí dodaných „skills".

Klíčový scénář propojení: uživatel má otevřené GUI, v Claude Code pošle PDF
s technickou zprávou střechy a požádá o zpracování přes Tectoru. Claude přečte
PDF, poskládá projekt přes CLI a **GUI se v reálném čase samo překreslí
a přepočítá** — uživatel okamžitě vidí výsledek.

---

## 2. Rozsah a fázování

| Fáze | Obsah | Stav |
|---|---|---|
| **Fáze 1** | Desktop GUI + CLI, kreslení polygonu, automatická střecha, výpočty, 3D, Excel export, **knihovna zakázek (SQLite) + úvodní stránka + poloha**, jazyky cs/sk/en. **CLI-ready** = jádro i kontrakt připravené pro AI. | tato specifikace |
| **Fáze 2** | AI CLI režim — definované „skills" pro Claude Code, workflow PDF → spec → výpočet, živá aktualizace GUI. | navazuje |
| **Fáze 3** | Rozšíření — různé sklony na hranu (weighted skeleton), přesahy okapů, vikýře, PDF report, materiálové počty. | budoucí |

**Pořadí dodávek ve Fázi 1 (TDD, inkrementálně, zevnitř ven):**
domén­ní jádro → aplikační use-casy + CLI → GUI 2D plátno + výsledky → Excel export → 3D náhled → i18n → živá synchronizace.

---

## 3. Glosář (střešní pojmy)

| Pojem | EN | Definice |
|---|---|---|
| **Půdorys** | footprint | Polygon obrysu budovy v rovině země (metry). |
| **Sklon** | pitch | Úhel sklonové roviny vůči vodorovné rovině (stupně). |
| **Okap / okapová hrana** | eave | Spodní vodorovná hrana sklonové roviny; po ní teče voda do žlabu. |
| **Štít / krajová hrana** | gable / rake | Svislá štítová stěna (trojúhelník/lichoběžník); její šikmé hrany nesou krajové tašky. |
| **Hřeben** | ridge | Vodorovná horní hrana, kde se potkávají dvě protilehlé roviny. |
| **Nároží** | hip | Vnější (konvexní) šikmá hrana, kde se potkávají dvě roviny směrem dolů k okapu. |
| **Úžlabí** | valley | Vnitřní (konkávní) šikmá hrana mezi dvěma rovinami; vzniká u tvarů L/T/U. |
| **Straight skeleton** | — | Algoritmus, který z polygonu odvodí osnovu střechy (hřebeny, nároží, úžlabí). |

---

## 4. Aktéři a režimy

- **Projektant / pokrývač (GUI)** — kreslí, nastavuje, čte výsledky, exportuje.
- **AI agent (Claude Code)** — řídí CLI, čte PDF, generuje/upravuje projekt. Není
  součástí Tectory; Tectora mu poskytuje deterministický nástroj a „skills".

---

## 5. Funkční požadavky

### 5.1 Zadání půdorysu (plátno)

- **FR-1** Uživatel nakreslí na 2D plátno uzavřený polygon (obdélník, L, T, U
  i obecný jednoduchý polygon bez sebeprůniků).
- **FR-2** Body polygonu lze přidávat, mazat a táhnout; souřadnice se zobrazují
  v **metrech**.
- **FR-3** Plátno má **mřížku** a volitelný **snap** na mřížku pro přesné kreslení.
- **FR-4** Uživatel může zadat rozměry i číselně (délky hran), nejen myší.
- **FR-5** Aplikace ohlídá platnost polygonu (uzavřený, bez sebeprůniku, ≥ 3 body)
  a srozumitelně hlásí chybu.

### 5.2 Hrany a sklon

- **FR-6** Každá hrana polygonu má **typ**: `okap` nebo `štít`. Výchozí = okap.
- **FR-7** Uživatel přepíná typ hrany kliknutím na hranu na plátně.
- **FR-8** Uživatel nastaví **jeden globální sklon** střechy. Jednotka: stupně
  (volitelně zobrazení i v %). Rozsah 1–89°.
- **FR-9** Kombinace typů hran určuje typ střechy automaticky:
  - 1 okap = **pultová**; 2 protilehlé okapy + 2 štíty = **sedlová**;
    4 okapy = **valbová**; čtvercový půdorys + 4 okapy = **stanová**;
    sedlová s useknutými konci = **polovalbová**.
  - (Různé sklony na jednotlivé hrany jsou mimo Fázi 1 — viz §11.)

### 5.3 Generování střechy

- **FR-10** Z půdorysu, typů hran a sklonu aplikace **automaticky vygeneruje**
  roviny střechy a osnovu (hřeben/nároží/úžlabí) pomocí straight-skeleton.
- **FR-11** Generování je **deterministické** — stejný vstup dá vždy stejný výstup.

### 5.4 Výpočty (viz přesné definice v §9)

- **FR-12** Aplikace spočítá a zobrazí:
  - **Plocha střechy** (m²) — celkem i po jednotlivých rovinách (skutečná
    skloněná plocha, ne průmět).
  - **Délka hřebene** (m).
  - **Délka nároží** (m) a **počet nároží**.
  - **Délka úžlabí** (m) a **počet úžlabí**.
  - **Obvod okapové hrany** (m).
  - **Délka krajových (štítových) hran** — zvlášť **levá** a **pravá** (m).
- **FR-13** Výpočet se spustí tlačítkem **„Provést výpočet"** a/nebo automaticky
  po změně vstupu (živý režim).
- **FR-14** Výsledky se zobrazí přehledně s jednotkami; lze je zkopírovat.

### 5.5 3D model

- **FR-15** Aplikace zobrazí **3D model** vygenerované střechy (PyVista),
  s otáčením a přiblížením.
- **FR-16** 3D pohled barevně rozliší roviny a typy hran (hřeben/nároží/úžlabí/okap).
- **FR-17** Přepínání mezi 2D plátnem a 3D náhledem.

### 5.6 Projekt a Excel

- **FR-18** Projekt se ukládá/načítá jako **JSON** (`.tectora`/`.json`) — jediný
  zdroj pravdy (viz §8).
- **FR-19** Výsledky lze **exportovat do Excelu** (.xlsx) jako přehledný výkaz výměr
  (totály + rozpis po rovinách).
- **FR-20** Nový / Otevřít / Uložit / Uložit jako.

### 5.7 Jazyky (i18n)

- **FR-21** UI plně v **češtině, slovenštině a angličtině**; přepínač jazyka.
- **FR-22** Všechny texty přes překladový katalog (žádné natvrdo zadané řetězce).

### 5.8 CLI (CLI-ready jádro)

- **FR-23** CLI umí: vytvořit projekt, nastavit polygon, označit typy hran,
  nastavit sklon, **spustit výpočet**, exportovat výkaz, vypsat shrnutí.
- **FR-24** CLI pracuje nad stejným JSON projektem jako GUI a používá stejné jádro.
- **FR-25** CLI má strojově čitelný výstup (JSON) i čitelné shrnutí pro člověka.
- **FR-26** Repozitář obsahuje **„skills"/instrukce** (`docs/ai/`) popisující CLI
  a JSON schéma tak, aby s tím Claude Code uměl spolehlivě pracovat.

### 5.9 Živá synchronizace GUI ↔ CLI

- **FR-27** GUI **sleduje projektový soubor** (file-watcher). Když ho změní CLI
  (např. řízené Claude Code), GUI se **automaticky znovu načte, přepočítá
  a překreslí** (2D i 3D) bez zásahu uživatele.
- **FR-28** Konflikt zápisu (GUI i CLI naráz) se řeší bezpečně — poslední zápis
  na disk vyhrává, GUI varuje při neuložených změnách.

### 5.10 Knihovna zakázek a úvodní stránka

- **FR-29** Aplikace má **lokální databázi (SQLite)** v adresáři aplikačních dat
  (Windows `%APPDATA%/Tectora/tectora.db`) pro ukládání zakázek (uložených nákresů).
- **FR-30** **Úvodní stránka**: tlačítko **„Nové"** + **kolekce** historických
  zakázek (karty/řádky se základním přehledem: název, poloha, datum, plocha).
- **FR-31** Nad zakázkami plné **CRUD**: vytvořit, otevřít, upravit, smazat.
- **FR-32** Každá zakázka uchovává: **název**, **poznámku**, volitelnou **polohu**
  (název místa + souřadnice), časy vytvoření/úpravy, samotný **projekt (JSON)**
  a **naposledy spočítaný výsledek** (pro náhled v seznamu bez přepočtu).
- **FR-33** U každé položky **kontextové menu „tři tečky"**: Otevřít, Přejmenovat,
  Upravit (název/poznámka/poloha), Duplikovat, Smazat.
- **FR-34** Knihovna je **plně ovladatelná i z CLI** (CRUD) → Claude Code může
  zakázky zakládat/číst/měnit/mazat; nová zakázka od Claude se objeví na úvodní
  stránce (díky živé synchronizaci, FR-27).

### 5.11 Poloha (našeptávač míst)

- **FR-35** Pole polohy je **našeptávač**: uživatel píše (např. „Mor") a aplikace
  online nabídne **seznam návrhů míst** (Moravský Krumlov, Moravské Budějovice…);
  výběrem se uloží **název místa + souřadnice**.
- **FR-36** Našeptávání přes **OpenStreetMap / Nominatim** (bez API klíče, s ohledem
  na jejich usage policy: identifikace User-Agentem, rozumný rate-limit). **Offline**
  se pole nenašeptává a poloha jde vyplnit/ponechat ručně (poloha je **volitelná**).
- **FR-37** **Geokódování je dostupné i z CLI** (příkaz `geocode`) pro AI agenta.

---

## 6. Nefunkční požadavky

- **NFR-1 (Architektura)** Hexagonální architektura (Ports & Adapters), viz §7.
  Veškerá výpočetní logika je v doméně bez závislosti na GUI/frameworcích; GUI
  i CLI jsou tenké adaptéry nad use-casy.
- **NFR-2 (TDD)** Vše vzniká testy napřed. Doména a I/O mají vysoké pokrytí, včetně
  **property-based** testů na geometrické invarianty a **golden-file** testů na
  ručně spočítané střechy. CLI se testuje přes subprocess + golden JSON.
- **NFR-3 (Determinismus)** Stejný vstup → stejný výstup (důležité pro testy i AI).
- **NFR-4 (Přesnost)** Délky/plochy s tolerancí ≤ 1 mm / ≤ 0,001 m² vůči
  referenčnímu výpočtu; zaokrouhlení jen při zobrazení.
- **NFR-5 (Jednotky)** Interně i navenek metry, m², stupně. Žádné jiné jednotky.
- **NFR-6 (Platforma)** Primárně Windows 11; stack (PySide6, PyVista) je
  multiplatformní, takže Linux/macOS nejsou blokované.
- **NFR-7 (Použitelnost)** Minimalistické UI — jen funkce, které se reálně použijí.
- **NFR-8 (Výkon)** Výpočet a překreslení běžné střechy do ~200 ms; živá
  aktualizace po změně souboru do ~500 ms.
- **NFR-9 (Robustnost)** Neplatný vstup nikdy nespadne — vrací srozumitelnou chybu
  (v GUI hláška, v CLI nenulový exit kód + JSON s chybou).
- **NFR-10 (Síť & soukromí)** Geokódování má timeout a běží mimo UI vlákno; při
  výpadku/offline aplikace degraduje (poloha zůstane prázdná, žádné zaseknutí).
  Žádné automatické sledování polohy uživatele — poloha se zjišťuje jen na základě
  toho, co uživatel napíše.

---

## 7. Architektura — Hexagonální (Ports & Adapters)

Tectora používá **hexagonální architekturu**, pragmatickou realizaci Clean
Architecture. Základní pravidlo: **závislosti míří jen dovnitř.** Doména nezná
Qt, openpyxl ani souborový systém. Frameworky a I/O jsou „venku" jako adaptéry
připojené přes porty (rozhraní).

### 7.1 Vrstvy

- **domain** — entity a čistá logika: půdorys, straight-skeleton, roviny střechy,
  výměry. Závisí maximálně na `numpy`. Žádné frameworky, žádné I/O.
- **application** — use-casy (orchestrace) + **porty** (rozhraní). Závisí jen na
  doméně. Definuje, *co* aplikace umí, ne *jak* se to ukládá/zobrazuje.
- **adapters / cli / gui** — vnější vrstva. *Driving* adaptéry (GUI, CLI, AI přes
  CLI) volají use-casy; *driven* adaptéry (JSON repo, Excel, file-watcher)
  implementují porty.

### 7.2 Pravidlo závislostí

```
   adapters / cli / gui  ──volá──►  application  ──používá──►  domain
   (Qt, CLI, JSON, Excel,           (use-casy +              (čisté entity
    watcher)          ◄─implementuje─ porty)                  + geometrie)
                         porty
```

Doména a application **nikdy** neimportují z adapters/cli/gui. Závislost na
konkrétní technologii se obrací přes port: use-case zná `ProjectRepository`
(rozhraní), ne `json_repository` (implementaci).

### 7.3 Struktura balíčků (`src/tectora/`)

```
domain/
  footprint.py     # Footprint, Edge, EdgeType(eave/gable) + validace
  skeleton.py      # straight-skeleton (pure)
  roof.py          # RoofPlane, klasifikace hran: ridge/hip/valley
  measurements.py  # plocha, hřeben, nároží, úžlabí, okap, krajová L/P
  location.py      # Location (název místa, lat, lon) — value object
  units.py         # jednotky, převody úhlů
  errors.py        # InvalidPolygon, PitchOutOfRange…

application/
  project.py       # agregát projektu (footprint+edges+pitch) ↔ JSON
  library.py       # LibraryEntry (projekt + metadata + výsledek + časy)
  use_cases.py     # CreateProject, SetFootprint, SetEdgeType, SetPitch,
                   #   CalculateRoof, ExportReport, Save/Load/List/Delete entry
  ports.py         # Protokoly: ProjectRepository, ReportExporter,
                   #   ProjectLibrary, GeocodingService
  dto.py           # ResultDTO (serializovatelné výstupy)

adapters/
  json_repository.py   # ProjectRepository: load/save JSON (schéma v1.0)
  excel_exporter.py    # ReportExporter: výkaz .xlsx (openpyxl)
  sqlite_library.py    # ProjectLibrary: SQLite knihovna zakázek (CRUD)
  nominatim_geocoder.py# GeocodingService: našeptávač míst (OSM/Nominatim)
  file_watcher.py      # živá sync (QFileSystemWatcher / watchdog)

cli/main.py        # driving adaptér → use-casy   (řídí ho Claude Code)
gui/               # driving adaptér (PySide6): canvas2d, view3d (PyVista), panely
i18n/              # katalogy cs/sk/en
```

### 7.4 Porty (rozhraní)

Definované přes `typing.Protocol` (lehké, bez dědičné obřadnosti). Zavádíme je jen
tam, kde pomáhají testům/výměně:

- **`ProjectRepository`** — `load(path) -> Project`, `save(path, project) -> None`.
- **`ReportExporter`** — `export(result, path) -> None`.
- **`ProjectLibrary`** — `list()`, `get(id)`, `add(entry) -> id`,
  `update(id, entry)`, `delete(id)` nad knihovnou zakázek.
- **`GeocodingService`** — `search(query) -> list[Place]` pro našeptávač polohy.

Geometrie je čistá doména → žádný port okolo ní (YAGNI).

### 7.5 Mapování požadavků na vrstvy

| Požadavek | Vrstva |
|---|---|
| Výpočty (straight-skeleton, výměry) | `domain` — pure, plně TDD bez Qt |
| GUI i CLI dělají totéž | tenké **driving adaptéry** nad stejnými use-casy |
| AI řídí CLI | Claude Code = další driving adaptér, beze změny jádra |
| Excel / JSON | **driven adaptéry** za porty → vyměnitelné, testovatelné |
| Knihovna zakázek / poloha | `sqlite_library` / `nominatim_geocoder` = driven adaptéry za porty |
| Živá sync | `file_watcher` adaptér spustí use-case „reload" v GUI |

### 7.6 Dopad na testování (TDD)

- **domain** — čistě jednotkové testy (property-based + golden-file), bez Qt/IO.
- **application** — use-casy s falešnými (fake) implementacemi portů.
- **adapters** — integrační testy (skutečný JSON/Excel/soubor).
- **gui** — tenké, jen smoke testy; logika je v doméně.

---

## 8. Datový model — JSON kontrakt (zdroj pravdy)

Projekt = jeden JSON soubor. Tohle je zároveň kontrakt pro AI.

```json
{
  "schema_version": "1.0",
  "project_name": "Rodinný dům",
  "units": "m",
  "footprint": {
    "vertices": [[0, 0], [12, 0], [12, 8], [0, 8]],
    "edges": [
      { "type": "eave" },
      { "type": "gable" },
      { "type": "eave" },
      { "type": "gable" }
    ]
  },
  "global_pitch_deg": 35.0,
  "overhang_m": 0.0
}
```

- `vertices` — body polygonu v metrech, v pořadí (proti směru hodin); polygon je
  uzavřený implicitně (poslední → první).
- `edges[i]` — typ hrany mezi `vertices[i]` a `vertices[i+1]`. `type` ∈
  `{"eave", "gable"}`. (Pole `pitch_deg` na hraně je rezervované pro Fázi 3;
  ve Fázi 1 se nepoužívá.)
- `global_pitch_deg` — sklon všech sklonových rovin.
- `overhang_m` — přesah okapu; ve Fázi 1 vždy `0.0`.

**Výsledek výpočtu (JSON):**

```json
{
  "schema_version": "1.0",
  "units": { "length": "m", "area": "m2", "angle": "deg" },
  "totals": {
    "area_m2": 116.95,
    "ridge_length_m": 4.0,
    "hip_length_m": 0.0,
    "hip_count": 0,
    "valley_length_m": 0.0,
    "valley_count": 0,
    "eave_perimeter_m": 24.0,
    "rake_left_m": 9.76,
    "rake_right_m": 9.76
  },
  "planes": [
    { "id": 1, "area_m2": 58.47, "pitch_deg": 35.0, "azimuth_deg": 0.0 },
    { "id": 2, "area_m2": 58.47, "pitch_deg": 35.0, "azimuth_deg": 180.0 }
  ]
}
```

---

## 9. Přesné definice měřených veličin

Osnova střechy (skeleton) se nejprve **zvedne do 3D**: každý vnitřní uzel dostane
výšku `vzdálenost_od_nejbližší_okapové_hrany × tan(sklon)`. Délky se měří jako
**skutečné 3D délky** podél střechy.

- **Plocha roviny** = plocha jejího půdorysného regionu / `cos(sklon)`.
  **Plocha střechy** = součet ploch všech rovin (skutečná skloněná plocha).
- **Délka hřebene** = součet 3D délek vodorovných hřebenových hran (oba krajní
  uzly ve stejné výšce, leží mezi dvěma protilehlými rovinami).
- **Nároží** = vnější šikmé hrany skeletu (konvexní roh). **Délka** = součet jejich
  3D délek; **počet** = jejich počet.
- **Úžlabí** = vnitřní šikmé hrany skeletu (konkávní roh, u L/T/U). **Délka**
  a **počet** analogicky.
- **Obvod okapové hrany** = součet délek hran půdorysu označených jako `okap`
  (vodorovné, takže 3D = 2D délka).
- **Krajové (štítové) hrany** = šikmé hrany střešních rovin nad hranami označenými
  jako `štít` (3D délky). Rozdělení **levá/pravá**: hrana je *levá*, pokud rovina,
  ke které patří, klesá při pohledu z okapu nahoru doleva; jinak *pravá*.
  (Toto je konvence pro rozlišení levých/pravých krajových tašek; ověří se
  golden-file testy a případně doladí.)

---

## 10. CLI kontrakt (přehled)

```bash
tectora new projekt.json --name "Rodinný dům"
tectora set-footprint projekt.json --vertices "0,0; 12,0; 12,8; 0,8"
tectora set-edge projekt.json --index 1 --type gable
tectora set-edge projekt.json --index 3 --type gable
tectora set-pitch projekt.json --deg 35
tectora calculate projekt.json --output vysledek.json
tectora export projekt.json --xlsx vykaz.xlsx
tectora show projekt.json          # čitelné shrnutí

# Knihovna zakázek (SQLite) — plný CRUD pro člověka i AI
tectora library list                              # JSON seznam zakázek
tectora library add projekt.json --name "Dům Novák" --note "valba" --location "Brno"
tectora library get 7 --output projekt.json       # vytáhne projekt zakázky #7
tectora library update 7 --name "Dům Novákovi" --note "upraveno"
tectora library delete 7

# Geokódování (našeptávač) pro AI
tectora geocode "Mor"                             # JSON návrhy: název + lat/lon
```

- Každý příkaz čte/zapisuje stejný JSON → GUI to díky file-watcheru zobrazí živě.
- Chyba = nenulový exit kód + `{ "error": { "code", "message" } }` na stdout.
- Přesný popis pro AI agenta je v `docs/ai/` (Fáze 2).

---

## 11. Mimo rozsah Fáze 1 (YAGNI)

- Různé sklony na jednotlivé hrany (weighted straight skeleton) → Fáze 3.
- Přesahy okapů, vikýře, světlíky, komíny → Fáze 3.
- Materiálové počty (počet tašek, latě, fólie) → Fáze 3.
- PDF report (výstup) → Fáze 3 (vstupní PDF řeší AI ve Fázi 2).
- Import zadání z Excelu (vstup = kreslení v GUI nebo PDF přes AI) → mimo rozsah.
- Tectora sama nevolá žádné LLM API — AI běží vně (Claude Code) a řídí CLI.

---

## 12. Akceptační kritéria (golden cases)

1. **Sedlová 12 × 8 m @ 35°** (2 okapy, 2 štíty): plocha, hřeben = 12 m,
   nároží = 0, úžlabí = 0, okap = 24 m, krajové levé = pravé. (Referenční hodnoty
   se dopočítají v testu.)
2. **Valbová 12 × 8 m @ 35°** (4 okapy): 4 nároží, kratší hřeben, žádné štíty.
3. **Stanová 8 × 8 m @ 35°** (4 okapy, čtverec): 4 nároží, hřeben = 0 (vrchol).
4. **L-tvar @ 35°**: vznikne ≥ 1 úžlabí; `valley_count ≥ 1`, `valley_length > 0`.
5. **Pultová** (1 okap): 1 rovina, žádný hřeben/nároží/úžlabí.
6. **CLI = GUI**: stejný projekt dá přes CLI i GUI identické výsledky.
7. **Živá sync**: změna JSON přes CLI se v otevřeném GUI projeví do 500 ms.
8. **Knihovna CRUD**: `library add` → `list` ukáže záznam → `get` vrátí stejný
   projekt → `update` změní název → `delete` ho odebere. CLI i GUI nad stejnou DB.
9. **Geokódování**: `geocode "Mor"` vrátí ≥ 1 návrh s názvem a souřadnicemi;
   offline vrátí prázdný seznam bez pádu.

---

## 13. Otevřené otázky / předpoklady

- Konvence levá/pravá krajová hrana (§9) — potvrdit na reálných příkladech.
- Existující `README.md` popisuje Fázi 1 jako pouze obdélníkovou (úžlabí = 0);
  po odsouhlasení této specifikace ho srovnáme (Fáze 1 = polygon + úžlabí).
- Formát projektového souboru: přípona `.tectora` (JSON) vs prosté `.json` —
  default `.tectora`.
```
