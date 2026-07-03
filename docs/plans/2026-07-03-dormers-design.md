# Vikýře (Gaube) — návrh implementace

Stav: **návrh, neimplementováno.** Tato větev obsahuje jen tento dokument —
implementace je větší téma a zaslouží si samostatnou, soustředěnou session.

## Cíl

Sedlový (příp. pultový) vikýř posazený na střešní rovinu rodičovské budovy,
po vzoru DACAD/RISO-Star: vikýř zmenšuje plochu rodičovské roviny (výřez),
přidává vlastní plochy a generuje úžlabí tam, kde jeho střecha protíná
rodičovskou rovinu.

## Proč to nejde „zadarmo" přes multi-building

Dnešní model už umí vikýř **vizuálně** (malá budova s `base_height_m` nad
rodičem — od větve s výškou budovy), ale výkaz je špatně:

1. plocha rodičovské roviny se nezmenší o výřez,
2. úžlabí vikýř×rodič se nespočítá,
3. okap vikýře se počítá jako plnohodnotný okap, i když jeho spodní hrana
   leží na rodičovské rovině.

## Navržený model

Nový objekt `Dormer` **vlastněný budovou** (ne samostatná budova):

```python
@dataclass
class Dormer:
    parent_face: str          # jméno roviny rodiče (z FaceResult.name)
    offset_along_eave_m: float  # pozice podél okapu rodičovské roviny
    width_m: float            # šířka vikýře (podél okapu)
    depth_m: float            # hloubka (kolmo do střechy)
    pitch_deg: float          # sklon stříšky vikýře
    sill_height_m: float      # výška čela nad rodičovskou rovinou
    type: str = "gable"       # gable | shed
```

`Building.dormers: list[Dormer]` — serializace stejně jako ostatní parametry
(schema 2.x, výchozí prázdný seznam → staré soubory se načtou beze změny).

## Geometrie (fáze výpočtu)

Vše se odehrává **v rovině rodičovské plochy** (2D úloha v šikmé rovině):

1. Rodičovskou rovinu rozvineme do 2D (známe sklon i orientaci).
2. Vikýř do ní promítne svůj obrys: obdélník čela + dvě šikmé stříšky.
3. Průnik obrysu s rovinou = **výřez** (odečíst od plochy roviny) a jeho šikmé
   hrany = **úžlabí** (přičíst do `valley_length_m`).
4. Plochy vikýře (2 roviny stříšky + čelo + boční stěny) spočítat parametricky
   — vikýř je malý „gable" obdélník, lze použít stávající exact calculator
   s useknutím spodní části v místě průniku.

Kroky 2–3 jsou polygon clipping **v jedné rovině** → stačí Sutherland–Hodgman
(výřez je vůči rozvinuté rovině konvexní obdélník/pětiúhelník) — žádný obecný
3D boolean. `core/geom2d.py` už má základ (průsečíky, point-in-polygon).

## Ověření (TDD)

- Golden: vikýř 2×1,5 m na sedlové 10×6/35° — ruční výpočet výřezu, úžlabí
  a ploch (dvě nezávislé cesty: parametricky vs. clipping).
- Vlastnost: Σ ploch (rodič − výřez + vikýř) ≥ původní plocha; úžlabí = 2 ks.
- Vikýř mimo rovinu / přes hřeben → validace s jasnou chybou.

## UI (minimální)

- Tlačítko „+ Vikýř" po kliknutí na rovinu (stejný hit-test jako „Sklon
  plochy"), parametry v panelu (šířka / hloubka / sklon / výška čela).
- 2D: obrys vikýře na rodičovské rovině + úžlabí fialově.
- 3D: stříška + čelo z existujících face-quadů, výřez rodiče přes
  `skirt_walls`-styl boundary handling.

## Odhad

Jádro (rozvinutí roviny + clipping + výkaz): ~2–3 dny s testy.
UI (klik-umístění + panel + 2D/3D render): ~2 dny.
Doporučené pořadí: jádro nejdřív (CLI/testy), UI až nad ověřeným výpočtem.
