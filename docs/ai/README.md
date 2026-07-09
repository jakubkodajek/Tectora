# Tectora × AI — jak s aplikací pracuje agent

Tectora je **deterministická** aplikace (GUI + CLI). Sama nikdy nevolá žádné
LLM API — obráceně: **AI agent (Claude) řídí Tectoru přes CLI**. Typický
scénář: uživatel pošle agentovi PDF s projektovou dokumentací, agent z ní
vyčte půdorysy a parametry, sestaví projektový JSON, zvaliduje ho, uloží do
knihovny zakázek — a zakázka se objeví na domovské obrazovce GUI.

## Mapa dokumentů

| Soubor | K čemu |
|---|---|
| [tectora-cli.md](tectora-cli.md) | Kompletní reference všech CLI příkazů (nástrojů) |
| [skill-projekt-z-podkladu.md](skill-projekt-z-podkladu.md) | Pracovní postup: z podkladů (PDF/výkres) → hotová zakázka |

## Zásady pro agenta

1. **Formát projektu zjišťuj z `tectora schema --project`** — ne z paměti.
   Schéma je anotované (česky) a je jediným zdrojem pravdy.
2. **Před uložením vždy `tectora validate`.** Vrací konkrétní problémy
   (špatný štít, protínající se polygon, sklon mimo rozsah…) — oprav vstup
   a validuj znovu, dokud není exit code 0.
3. **Jednotky:** metry, m², stupně. Souřadnice y roste „nahoru" (matematická
   konvence, ne obrazovková).
4. **Kreslený polygon je obrys STŘECHY** (okapová hrana), ne fasáda. Přesah
   okapu (`overhang_m`) jen odsazuje stěny dovnitř v 3D náhledu.
5. **Složité střechy skládej z více budov** (kompoziční model): L-ka se
   dvěma výškami = dvě obdélníkové budovy; věž = menší budova s vyšším
   `eave_height_m`. Překryv půdorysů je dovolen (validate ho hlásí jen jako
   upozornění), ale překrytá plocha je v součtech dvakrát.
6. **Štít (`gable`) jde jen na konec křídla s kolmými sousedními hranami** —
   na obdélníku typicky obě kratší strany. Jinam patří `eave` (výpočet tam
   udělá valbu/nároží sám).
7. Všechny příkazy při chybě vrací **exit code 2** a česky popsanou chybu na
   stderr — čti ji a reaguj, neopakuj slepě stejný příkaz.

## Sdílení dat s GUI

- **Knihovna zakázek** = SQLite v `%APPDATA%/Tectora/tectora.db`. CLI
  (`tectora library …`) a GUI pracují nad toutéž databází; co agent uloží,
  uživatel vidí na domovské obrazovce (po jejím obnovení).
- **Přenositelný soubor** `.tectora` = čistý JSON projektu (+ volitelně
  `note`, `location`) — vhodný na předání konkrétního návrhu.
