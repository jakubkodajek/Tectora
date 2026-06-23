# Tectora — Design brief (pro návrh UI)

> Podklad pro vytvoření vizuálního návrhu (např. v Claude / design nástroji).
> Cíl: **jednoduchý, minimalistický, čistý** desktopový interface. Inspirace
> rozložením z klasického střešního CAD (viz referenční screenshot), ale
> **bez přetížených panelů** — jen nástroje, které uživatel reálně použije.

---

## 1. Princip

- **Minimalismus a srozumitelnost.** Žádné desítky ikon „pro jistotu". Na ploše
  je jen to, co patří k workflow: vyber/založ zakázku → nakresli → označ hrany →
  nastav sklon → **Provést výpočet** → výsledky → export.
- **Jeden hlavní úkol na obrazovce.** Plátno je hrdina, vše ostatní ho podporuje.
- **Klid a vzdušnost.** Hodně bílého prostoru, jemná mřížka, jeden akcentní tón.
- **Trojjazyčnost (cs/sk/en).** Texty krátké, ať se vejdou ve všech jazycích;
  přepínač jazyka snadno dostupný.

Aplikace má **dvě hlavní obrazovky**: úvodní stránku (knihovna zakázek) a editor.

---

## 2. Úvodní stránka (knihovna zakázek)

Vstupní obrazovka. Velké tlačítko/karta **„Nové"** a pod ním **kolekce uložených
zakázek** (historie výpočtů).

```
┌─────────────────────────────────────────────────────────────┐
│  Tectora                              [Hledat…]   Jazyk ▾      │
├─────────────────────────────────────────────────────────────┤
│   ┌───────────────┐                                           │
│   │      +        │   Nové                                     │
│   │   (nová)      │                                           │
│   └───────────────┘                                           │
│                                                               │
│   Moje zakázky                                                │
│   ┌──────────────────┐  ┌──────────────────┐  ┌────────────┐ │
│   │ [náhled střechy] ⋮│  │ [náhled střechy] ⋮│  │ …         ⋮│ │
│   │ Dům Novák         │  │ Chata Brno        │  │            │ │
│   │ Brno · 12.6.2026  │  │ Telč · 3.5.2026   │  │            │ │
│   │ 117 m²            │  │ 84 m²             │  │            │ │
│   └──────────────────┘  └──────────────────┘  └────────────┘ │
└─────────────────────────────────────────────────────────────┘
```

- **Karta zakázky** = malý náhled střechy (2D osnova), **název**, **poloha · datum**,
  klíčová hodnota (plocha). Klik = otevřít v editoru.
- **Tři tečky (⋮)** v rohu karty = kontextové menu: **Otevřít, Přejmenovat,
  Upravit (název/poznámka/poloha), Duplikovat, Smazat**. Smazání s potvrzením.
- **Hledat** filtruje podle názvu/polohy. Prázdný stav = vlídná výzva „Zatím žádné
  zakázky — začni tlačítkem Nové".

---

## 3. Editor (2D plátno)

```
┌─────────────────────────────────────────────────────────────┐
│  ← Zpět   Soubor · Úpravy · Zobrazení · Jazyk · Nápověda      │
├─────────────────────────────────────────────────────────────┤
│  Toolbar: [Kreslit polygon] [Upravit body] [Okap/Štít]        │
│  [Sklon: 35°▾]   ·   [2D | 3D]   ·   [Provést výpočet ▶]      │
├──────────────────────────────────────┬──────────────────────┤
│                                      │  ZAKÁZKA              │
│                                      │  Název: Dům Novák     │
│            PLÁTNO (2D)               │  Poloha: [Brno    🔎] │
│        mřížka + snap + polygon       │  Poznámka: […]        │
│        (přepne se na 3D náhled)      │  ──────────────────   │
│                                      │  PARAMETRY            │
│                                      │  • Sklon: 35°         │
│                                      │  • Typ střechy: auto  │
│                                      │  ──────────────────   │
│                                      │  VÝSLEDKY             │
│                                      │  Plocha     116,95 m² │
│                                      │  Hřeben       4,00 m  │
│                                      │  Nároží  0 ks / 0,0 m │
│                                      │  Úžlabí  0 ks / 0,0 m │
│                                      │  Okap        24,00 m  │
│                                      │  Krajová L    9,76 m  │
│                                      │  Krajová P    9,76 m  │
│                                      │  [Uložit] [Export XLS]│
├──────────────────────────────────────┴──────────────────────┤
│  Status bar:  Kurzor X: 4,20 m  Y: 2,10 m   ·   Jednotky: m   │
└─────────────────────────────────────────────────────────────┘
```

- **Toolbar** = max ~6–7 akcí, textové popisky + jednoduché čárové ikony.
- **Pravý panel** = tři sekce: *Zakázka* (název, poloha, poznámka), *Parametry*
  (vstupy), *Výsledky* (výstupy), oddělené jemnou linkou.
- **Pole Poloha = našeptávač:** uživatel píše „Mor", pod polem vyjede rozbalovací
  seznam návrhů (Moravský Krumlov, Moravské Budějovice…), klikem se vybere. Když
  je offline, pole se chová jako prosté textové (bez návrhů).
- **Tlačítko „Provést výpočet"** je výrazné (primární akce, akcentní barva).
- **Uložit** zapíše zakázku do knihovny; **Export XLS** uloží výkaz výměr.
- **Přepínač 2D/3D** je jeden segmentovaný přepínač.
- **Status bar** = živé souřadnice kurzoru v metrech + jednotky.

---

## 4. Plátno a vizuální jazyk geometrie

- Jemná **mřížka** (světle šedá), výraznější čára po každých 1 m / 5 m.
- **Polygon půdorysu**: tenká plná čára; uzly = malé kroužky (uchopitelné).
- **Typy hran odlišit barvou:** Okap = jedna barva (např. modrá), Štít = jiná
  (např. oranžová).
- **Osnova střechy (po výpočtu) s legendou:** Hřeben, Nároží, Úžlabí, Okap — každé
  jinou barvou; malá legenda v rohu plátna.
- **3D náhled**: světlé pozadí, plochy v neutrální barvě s jemným stínováním,
  hrany barevně dle stejné legendy jako 2D.

---

## 5. Barvy, typografie, tón

- **Světlé téma**, neutrální šedé tóny + **jeden akcent** (primární akce, aktivní
  nástroj). Tmavé téma je nice-to-have, ne nutnost.
- Bezpatkové, dobře čitelné písmo; čísla zarovnaná na desetinnou čárku.
- Ikony jednoduché, čárové (line icons), konzistentní tloušťka.
- Hodnoty výsledků vždy **s jednotkou** a na 2 desetinná místa.

---

## 6. Stavy obrazovek (states)

1. **Úvodní stránka — prázdná** (žádné zakázky) i **plná** (kolekce karet).
2. **Kontextové menu „tři tečky"** rozbalené nad kartou.
3. **Editor — prázdné plátno / onboarding** (výzva „Nakresli půdorys").
4. **Editor — kreslení/úprava** (aktivní nástroj zvýrazněný).
5. **Editor — po výpočtu** (osnova + vyplněné výsledky).
6. **Našeptávač polohy** rozbalený s návrhy.
7. **Chyba** (neplatný polygon, sklon mimo rozsah) — nenásilná hláška.
8. **Živá AI aktualizace** — jemná indikace „Aktualizováno" a překreslení.

---

## 7. Co NEdělat (poučení z přetíženého CAD)

- Žádný toolbar s 40 ikonami, kde uživatel netuší, co dělají.
- Žádné funkce mimo rozsah (vikýře, materiály, komíny — to je až Fáze 3).
- Žádné modální dialogy tam, kde stačí inline panel.
- Žádné jednotky kromě metrů/m²/stupňů.

---

## 8. Lokalizace — ukázka klíčových termínů

| CS | SK | EN |
|---|---|---|
| Nové | Nové | New |
| Moje zakázky | Moje zákazky | My projects |
| Poloha | Poloha | Location |
| Poznámka | Poznámka | Note |
| Plocha střechy | Plocha strechy | Roof area |
| Délka hřebene | Dĺžka hrebeňa | Ridge length |
| Nároží | Nárožie | Hip |
| Úžlabí | Úžľabie | Valley |
| Okap | Odkvap | Eave |
| Štít / Krajová hrana | Štít / Krajová hrana | Gable / Rake |
| Sklon | Sklon | Pitch |
| Provést výpočet | Vykonať výpočet | Calculate |
| Uložit | Uložiť | Save |
| Export do Excelu | Export do Excelu | Export to Excel |

---

## 9. Výstup, který od návrhu chceme

- **Úvodní stránka** — prázdný i plný stav, karta zakázky vč. rozbaleného „⋮" menu.
- **Editor** — prázdné plátno (onboarding) i stav po výpočtu.
- **Stav 3D náhledu.**
- **Pravý panel** (Zakázka + Parametry + Výsledky) v detailu.
- **Našeptávač polohy** rozbalený.
- **Paleta barev + typografie + sada ikon** (komponenty).
- Vše ve světlém tématu, minimalistické, připravené pro PySide6 (Qt).
```
