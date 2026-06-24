# Plán implementace — Obecný Straight Skeleton (self-written), střechy libovolného tvaru

> Verze 1.0 · 2026-06-24 · větev `feat/faze1-reconcile`.
> **Vlastní** pure-Python implementace straight skeletonu (Felkel–Obdržálek 1998),
> bez cizího/vendovaného kódu. Cíl: plný hip roof nad **libovolným prostým
> polygonem** (i šikmé úhly — baráky mají různé tvary) při jednotném sklonu;
> hřeben, nároží, **úžlabí** (reflexní rohy), 3D roviny, výměry. Výstup = stejný
> `RoofResult` jako stávající kalkulátor → GUI (G4/G4b) a CLI to rovnou využijí.

## Rozhodnutí (kontext)

- Obecný tvar je nutný (ne jen pravoúhlý) — reálné domy.
- Vendovat cizí SS je zablokováno (bezpečnost/supply-chain) a nečisté licenčně.
- → **Napíšeme algoritmus sami** podle veřejného popisu. Standardní, dobře
  zdokumentovaný algoritmus; implementace je naše.

## Algoritmus (shrinking wavefront / Felkel)

Polygon CCW. **LAV** = kruhový spojový seznam aktivních vrcholů; každý drží bod,
levou/pravou hranu (přímky), **bisector** (osa úhlu, směr dovnitř) a příznak
konvexní/reflexní. Vlnoplocha se posouvá dovnitř; vrcholy jedou po bisektorech.
Výška = ujetá vzdálenost `t`; z = `t·tan(sklon)`.

Eventy v prioritní frontě dle `t`:
- **Edge event**: protnou se bisektory dvou sousedních vrcholů → hrana mezi nimi
  zkolabuje → uzel kostry; odeber oba vrcholy, vlož sloučený s novým bisectorem.
- **Split event**: bisector **reflexního** vrcholu narazí na nesousední hranu →
  LAV se rozdělí na dvě smyčky → vzniká **úžlabí**.
Neplatné eventy (vrchol už zpracován) se přeskakují. Tvrdý strop iterací →
`RoofGenerationError` (anti-zacyklení).

## Soubory

```
src/tectora/core/
  geom2d.py            # vektory: sub/add/scale/dot/cross/norm/len, průniky přímka×přímka,
                       #   paprsek×úsečka, vzdálenost bod×přímka, orientace
  straight_skeleton.py # LAV, bisektory, edge+split eventy → SkeletonGraph (uzly+oblouky+face stopy)
  roof_from_skeleton.py# SkeletonGraph + footprint + pitch → RoofResult (3D roviny, klasifikace, výměry)
  roof_generator.py    # generate_roof(footprint, pitch) dispatch (viz SS5)
```

## Zlatá orákula (TDD se o ně opírá)

1. **Invariant ploch**: `Σ ploch rovin == plocha_půdorysu / cos(sklon)` — přesně,
   pro JAKÝKOLI tvar. Hlavní obrana proti chybám (hypothesis na náhodných polygonech).
2. **Obdélník**: SS výsledek ≈ stávající `tectora.core.calculator.calculate` (přesný).
3. **L golden**: konkrétní L ručně spočítané (valley/hip/ridge/area).
4. **Per-face**: součet ploch rovin == invariant; každá rovina je planární.

---

## Milník SS1 — geometrie + LAV + bisektory (pure, TDD)

**Files:** `tests/core/test_geom2d.py`, `src/tectora/core/geom2d.py`,
`tests/core/test_ss_lav.py`, `src/tectora/core/straight_skeleton.py` (jen LAV část).

- `geom2d.py`: čisté funkce na 2D vektorech (tuple `(x,y)`): `sub,add,scale,dot,
  cross,length,normalize`, `line_intersection(p1,d1,p2,d2)`, `ray_segment_intersection`,
  `dist_point_to_line`, `orientation(a,b,c)`. TDD na známých hodnotách.
- `straight_skeleton.py` (LAV část): `Vertex` (point, prev, next, edge_left dir,
  edge_right dir, bisector, is_reflex), `build_lav(polygon_ccw)` → kruhový seznam;
  `compute_bisector(v)`; reflex detekce z orientace.
- Test: CCW čtverec → 4 konvexní, bisektory míří do středu (45°). L (1 reflexní roh)
  → právě 1 `is_reflex`. Bisector přímého úhlu degeneruje korektně.
- Commit.

## Milník SS2 — edge eventy (konvexní polygony, libovolné úhly)

**Files:** rozšířit `straight_skeleton.py`; `tests/core/test_ss_convex.py`.

- Prioritní fronta edge eventů (čas = průnik sousedních bisektorů / kolaps hrany).
- Simulace: `skeletonize(polygon)` → `SkeletonGraph` (uzly s `t`, oblouky vrchol↔uzel,
  uzel↔uzel) pro polygony **bez reflexních rohů** (trojúhelník, konvexní n-úhelník,
  obdélník, čtverec).
- Test: trojúhelník → 1 vrcholový uzel; čtverec → 1 centrální uzel; obdélník →
  hřebenová hrana + 4 nárožní oblouky (polohy/časy odpovídají). Náhodné konvexní
  (hypothesis) → simulace doběhne, uzly uvnitř polygonu.
- Commit.

## Milník SS3 — split eventy (reflexní rohy, L/T/U i šikmé)

**Files:** rozšířit `straight_skeleton.py`; `tests/core/test_ss_reflex.py`.

- Detekce split eventu: reflexní vrchol vs nesousední hrany (čas/místo nárazu uvnitř
  pohybujícího se úseku). Rozdělení LAV na dvě smyčky; pokračování.
- Test: L → ≥1 úžlabní oblouk; „plus"/T tvar; ostrý reflexní roh (šikmý). Strop
  iterací → `RoofGenerationError`. Náhodné jednoduché polygony (hypothesis,
  generované jako monotónní/ortogonální i šikmé) → doběhne bez pádu.
- Commit.

## Milník SS4 — roof_from_skeleton → RoofResult

**Files:** `src/tectora/core/roof_from_skeleton.py`, `tests/core/test_roof_from_skeleton.py`.

- Během simulace stopuj **face per původní hranu**: každá hrana má uspořádaný
  seznam bodů své roviny (koncové body hrany + uzly kostry, jak je eventy přidávají).
- `build_roof(graph, footprint, pitch) -> RoofResult`: zvedni uzly do 3D
  (z=t·tanθ), z face-stop sestav 3D rovinu per hrana, plochy (3D), klasifikuj
  oblouky: vodorovný mezi 2 rovinami → **hřeben**; z konvexního vrcholu → **nároží**;
  z reflexního → **úžlabí**; spočítej délky (3D), počty, obvod okapu.
- Acceptance:
  - **invariant ploch** (hypothesis: náhodné konvexní polygony + parametrické L) přesně.
  - **obdélník**: `RoofResult` ≈ `calculate(RoofType.HIP/PYRAMID,...)` (plocha, hřeben,
    nároží len+count) do EPS.
  - **L golden**: konkrétní L → ručně ověřené `valley_length_m`, `hip_count`,
    `ridge_length_m`, `total_area_m2`.
- Commit.

## Milník SS5 — integrace `generate_roof` → `calculate_project`

**Files:** `src/tectora/core/roof_generator.py`; rozšířit `use_cases.py`; `tests/...`.

- `generate_roof(footprint, pitch)`: prostý polygon → SS cesta (libovolný tvar);
  (obdélník volitelně přes stávající přesný `calculate` jako rychlou/ověřenou
  zkratku — ale ověřit, že SS dává totéž). Nevalidní/ne-prostý → `RoofGenerationError`.
- `uc.calculate_project` použije `generate_roof` → GUI/CLI počítají libovolný tvar.
- e2e: L i šikmý polygon přes `calculate_project` → rozumný `RoofResult`,
  `valley_length_m > 0` u L; CLI nepadá.
- Commit.

## Milník SS6 — GUI úžlabí + ověření

**Files:** uprav `gui/editor/skeleton_render.py`; `tests/gui/...`.

- `classify_skeleton` rozliší úžlabí (barva `COL["valley"]` #0d9488) od nároží.
- Offscreen test: nakresli L, `_on_calculate`, `last_result.valley_length_m > 0`,
  na plátně ≥1 valley segment. Šikmý tvar → vykreslí se osnova i kóty.
- Commit.

## Hotovo, když

- `uc.calculate_project` zvládne **libovolný prostý polygon** (obdélník, L/T/U,
  šikmé tvary) s úžlabím, ve stejném `RoofResult`.
- Invariant ploch (hypothesis) + obdélníkový cross-check + L golden zelené.
- GUI editor nakreslí, spočítá a okótuje libovolný tvar střechy (jako tátův program).

## Pozn. / rizika

- **Numerická robustnost** je hlavní riziko obecného SS: důsledně `EPS`, zaokrouhlené
  klíče uzlů, ošetření simultánních/degenerovaných eventů; strop iterací →
  `RoofGenerationError` (GUI ukáže toast, nespadne).
- Stavíme **inkrementálně**: konvexní (edge eventy, SS2) ověřené invariantem dřív,
  než přijdou split eventy (SS3) — bug se chytí brzy.
- Štíty na obecných tvarech (weighted SS) = pozdější rozšíření; zatím všechny hrany
  okap (plný hip), což odpovídá tátovým příkladům.
