# Skill: Z podkladů (PDF/výkres) → zakázka v Tectoře

Postup pro agenta, který dostal projektovou dokumentaci (PDF s půdorysy,
řezy, pohledy — případně fotku náčrtu) a má z ní založit zakázku.

## 1. Co v podkladech hledat

| Údaj | Kde bývá | Do kterého pole |
|---|---|---|
| Obrys střechy (okapová hrana) | půdorys střechy / krovů | `footprint.vertices` |
| Kóty délek | půdorys (v mm → přepočti na m!) | rozměry vrcholů |
| Sklon střechy | řez, technická zpráva („sklon 35°") | `pitch_deg` |
| Štítové stěny | pohledy (svislá trojúhelníková stěna) | `edges[i].type = "gable"` |
| Výška okapu | řez (okap nad ±0,000) | `wall_height_m` (+ `base_height_m`) |
| Polovalba | pohled (useknutý štít) | `half_hip_m` |
| Různé sklony předo/zadní | řez | `pitch_back_deg` |
| Adresa stavby | razítko / technická zpráva | `--location` u `library add` |

**Pozor na jednotky:** české výkresy kótují v milimetrech. 12 450 → 12.45 m.

## 2. Sestavení půdorysu

- Souřadnice v metrech, y roste „na sever" výkresu. Začni v levém dolním
  rohu obrysu na (0, 0) a obcházej obrys **proti směru hodinových ručiček**.
- Kresli **obrys střechy** (vnější hranu okapu). Pokud výkres kótuje fasádu
  a přesah okapu zvlášť, přičti přesah k obrysu fasády a tentýž přesah zapiš
  do `overhang_m` (odsadí stěny v 3D).
- Vypusť zaoblení a drobné výstupky < ~0,2 m — na výkaz nemají vliv.
- `edges[i]` je hrana z vrcholu `i` do `i+1` (poslední zpět do prvního).
  Výchozí všude `"eave"`; `"gable"` jen tam, kde pohledy ukazují štítovou
  stěnu **na konci křídla s kolmými sousedy**.

## 3. Složitější střechy = více budov

Jedna budova = jedno střešní těleso s JEDNÍM hlavním sklonem. Skládej:

- **L-ka se stejnou výškou i sklonem** → jedna budova s L půdorysem
  (úžlabí/nároží dopočítá skeleton sám).
- **Křídla s různou výškou/sklonem** → dvě obdélníkové budovy vedle sebe
  (společná hrana = v pořádku, dotyk není překryv).
- **Věž / vyšší část nad nižší** → menší budova s `base_height_m` > 0.
- Kde si nejsi jistý rozkladem, udělej jednodušší variantu a do `--note`
  napiš, co je potřeba ověřit.

## 4. Ukázkový projekt (dům + garáž)

```json
{
  "schema_version": "2.0",
  "project_name": "Novák – RD Brno",
  "units": "m",
  "buildings": [
    {
      "name": "Dům",
      "footprint": {
        "vertices": [[0, 0], [12.4, 0], [12.4, 8.2], [0, 8.2]],
        "edges": [
          {"type": "eave"}, {"type": "gable"},
          {"type": "eave"}, {"type": "gable"}
        ]
      },
      "pitch_deg": 38.0,
      "overhang_m": 0.5,
      "wall_height_m": 5.8,
      "base_height_m": 0.0,
      "half_hip_m": 0.0,
      "pitch_back_deg": null
    },
    {
      "name": "Garáž",
      "footprint": {
        "vertices": [[12.4, 0], [18.4, 0], [18.4, 5.5], [12.4, 5.5]],
        "edges": [
          {"type": "eave"}, {"type": "eave"},
          {"type": "eave"}, {"type": "eave"}
        ]
      },
      "pitch_deg": 25.0,
      "overhang_m": 0.4,
      "wall_height_m": 2.7
    }
  ]
}
```

## 5. Validace a uložení (vždy v tomto pořadí)

```bash
tectora validate projekt.json          # opakuj po každé opravě, cíl exit 0
tectora calculate --project projekt.json   # čísla na kontrolu
tectora library add projekt.json --name "Novák – RD" \
    --note "Sklon z řezu A-A; garáž odhad, chybí kóta" --location "Brno"
```

## 6. Závěrečná kontrola vůči podkladům

1. **Plocha:** pokud dokumentace uvádí plochu střechy, porovnej s
   `totals.total_area_m2` — odchylka > 5 % ⇒ zkontroluj sklon a obrys.
2. **Výška hřebene:** `eave_height_m + results.apex_height_m` proti kótě
   hřebene v řezu.
3. **Tvar:** počty nároží/úžlabí v `results` proti půdorysu krovů.
4. Nesrovnalosti a odhady VŽDY vypiš uživateli a poznamenej do `--note` —
   výkaz jde do nabídky, mlčky odhadovat se nesmí.
