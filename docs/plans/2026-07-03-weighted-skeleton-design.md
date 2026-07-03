# Per-face sklon na obecných tvarech — vážený straight skeleton

Stav: **návrh, neimplementováno.** Tato větev obsahuje jen tento dokument.
Toto je nejrizikovější odložené téma — vlastní implementace skeletonu už
jednou byla opuštěna jako chybová (viz historie: přechod na vendorovaný
polyskel). Nezačínat bez rozmyslu.

## Cíl

Různý sklon pro KAŽDOU střešní rovinu obecného půdorysu (L/T/U…), ne jen
přední/zadní na obdélníku (dnešní `pitch_back`). Geometricky to znamená
**vážený straight skeleton**: hrana s menším sklonem „postupuje" dovnitř
rychleji (rychlost hrany ~ 1/tan(sklon)).

## Proč je to těžké

- Vážený SS potřebuje korektní **split eventy** s různými rychlostmi hran —
  přesně ta třída algoritmu, na které ztroskotala původní vlastní
  implementace (`core/straight_skeleton.py`, dnes nepoužívaná v produkci).
- Vendorovaný `polyskel` (Felkel) váhy **nepodporuje** a jeho úprava je
  netriviální zásah do cizího LGPL kódu.

## Doporučená cesta (v pořadí)

1. **Nejdřív zvážit, zda to vůbec je potřeba.** Multi-building kompozice +
   `pitch_back` pokrývá většinu reálných zakázek (DACAD to řeší stejně —
   tělesa, ne vážený skeleton). Zbývající případ: L-ka s křídly o různém
   sklonu → často jde složit ze dvou obdélníkových těles s překryvem.
2. **Externí knihovna, ne vlastní kód.** Kandidát: CGAL
   `create_weighted_straight_skeleton_2` přes balík `cgal` bindings, nebo
   `scikit-geometry` (obsahuje SS, váhy dle verze). Ověřit licenci (CGAL =
   GPL/komerční! — pozor) a dostupnost wheelů pro Windows.
   Alternativa: `polyskel-swig`/`botffy-polyskel` fork s váhami — prozkoumat.
3. Pokud knihovna nevyjde: **omezený vlastní algoritmus** jen pro
   „rectilinear" půdorysy (všechny hrany osové, L/T/U/kříž). Pro ně je vážený
   skeleton výrazně jednodušší (eventy jen na osových přímkách) a dá se
   otestovat golden testy proti ručním výpočtům. Toto je realistický scope.
4. Plnohodnotný obecný vážený SS s split eventy = poslední možnost;
   počítat s týdny práce a property-based testy od prvního dne
   (hypothesis: náhodné jednoduché polygony, invarianty počtů hran/uzlů,
   plocha roven součtu faces, žádné protínající se faces).

## Napojení na stávající kód

- `generate_roof(footprint, pitch)` → `generate_roof(footprint, pitches:
  list[float] | float)`; per-edge sklon už má kam uložit (`Edge`), UI hotové
  („Sklon plochy" klik) — jen dnes zapisuje globální/zadní hodnotu.
- `roof_from_skeleton.build_roof` má strukturální self-check (hips=konvexní,
  valleys=reflexní rohy) — pro vážený skeleton přestane platit 1:1, bude
  potřeba zobecnit (váhované úhly).

## Verdikt

Nedělat, dokud (a) nepřijde reálná zakázka, kterou nejde složit z těles,
a (b) není ověřená knihovna s kompatibilní licencí nebo odsouhlasený
rectilinear-only scope (bod 3).
