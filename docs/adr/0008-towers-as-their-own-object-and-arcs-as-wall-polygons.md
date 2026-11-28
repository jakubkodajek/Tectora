# ADR-0008: Towers are their own object; curved walls are polygons of short walls

- **Status:** Proposed (to be accepted by Jakub)
- **Date:** 2026-11-28
- **Deciders:** Jakub Kodajek

## Context

TERRAN gets drawings of roofs with a turret (a shaft through the roof with
a bell helm, an onion, a lantern and a spire), cones on round towers,
rounded building corners and semicircular ends (apses). Jakub asked
(2026-11-28) that such roofs and "the common modifications that turn up now
and then" can be calculated.

DACAD has no tower object. A cone or a polygonal spire is drawn as a part
whose walls follow the outline (a polygon of many short walls, every wall an
eave); a rounded outline is DACAD's Runddach: a polygon of short walls in
11.25° steps (15° where a step would be under 0.4 m) ([[dacad-feature-parity]],
`edit::round`). DACAD cannot stack tiers above each other, curve a profile,
or treat a lantern's vertical wall as anything but a 90° gable.

Since 2026-11-14 Jakub wants DACAD's results kept where DACAD works and
improved where it is impractical.

## Options considered

1. **Only DACAD's way (polygon parts).** A tower is a part with many walls
   and a high eave; tiers are pitch breaks. Pro: no new model, byte-exact
   with DACAD for a single cone or spire. Con: a pitch break cannot lean
   outward (an onion), cannot go vertical (a lantern) and cannot curve; the
   shaft does not cut its own roof out of the main roof; a round cone needs
   32+ walls that the user must keep in step. Not usable for the drawings.
2. **A tower object with a closed-form profile** (chosen). A convex plan
   shape (rectangle, regular polygon, circle, optionally with rounded
   corners) scaled about its axis at every height, and a stack of tiers each
   with a rise, a top width and a bulge (a circular arc in the profile:
   outward onion, inward bell). Areas are exact per outline edge
   ((k_a + k_b)/2 · l · sqrt(Δz² + (Δk·d)²), curved tiers in 128 chords); the
   shaft is cut out of the roofs it stands in as a convex chimney, so
   DACAD's chimney rules (plan area / cos(pitch), WV/WH/WS lines) apply.
   Pro: any turret of the drawings in one object, every number checkable by
   a closed form. Con: not DACAD; every tier has the base's plan shape (an
   octagonal lantern on a square shaft is approximated by the square).
3. **General 3D surfaces (a mesh editor).** Covers everything, checks
   nothing; far from a roofer's tool. Rejected.

For curved outlines the options were the same polygon (DACAD) or true arcs
in the core. True arcs would leave the DACAD core port (`dcore/`), which
only takes straight walls.

## Decision

We choose **option 2 for towers** and **DACAD's polygon for curved walls**:
`edit::arcs::round_corner` and `edit::arcs::bend_wall` turn a corner or a
wall into short walls with DACAD's Runddach steps, and the roof is the core's
roof on that polygon. A semicircular end is a bent wall with the bulge half
its length (a conical hip). A cone on a round building may be either a part
(DACAD's way) or a tower; the tower is the one the editor offers.

A tower's roof joins the totals, line totals (T, c, P, G), area by material
and by pitch, the price-sheet list (one roof per tower after the building
and dormer roofs, one face per roof tier, tiers leaning outward listed by
their slope from the horizontal) and the plan sheet. Its vertical tiers and
the shaft's walls are wall area, shown in the results, not in the roof
total nor in the list's `Stena` row (the building's walls).

## Consequences

- Positive: the drawings' turret, cones, octagonal spires, helms and onions
  calculate exactly (tests `tower_cases.rs`, `tower.rs`); curved walls stay
  on DACAD's path and its byte-exact core.
- Negative / risks: tower numbers have no DACAD fixture: they are checked
  by closed forms (cone π r s, frustum, hemisphere zone, triangulated
  surface) instead. The tower does not interact with other roof objects
  (a dormer under it, a tower straddling two buildings' join beyond the
  chimney rules). Schema version 4.
- Audit round 2 (2026-11-28) additions, no change of the decision:
  - The shaft's cut keeps the shaft's own symmetry (the same few chords on
    every rounded corner, a sharp corner where the arc is under 10 cm, a
    regular polygon of an even number of corners, at least four, for a
    circle or many short sides), so a rounded shaft has its front and back
    abutments WV/WH as a sharp one has.
  - A tower may have an eave `overhang`: its first tier runs on at its own
    pitch beyond the shaft, the eave `overhang · tan(pitch)` below
    `base_height`, as a wall's roof meets its eave at the overhang line;
    the shaft and its cut stay the wall's. Audit round 3 settled what the
    number means: the eave lies `overhang` out from every side, square to
    it (not the plan scaled, which put 1.5 m on the long sides of a 2 × 6 m
    tower for 0.5 m typed); each side's roof runs on at that side's pitch
    at the shaft (a curved tier along its tangent there), so a rectangle's
    steeper sides end lower and the steeper plane takes the eave's corner,
    as on a hip roof of two pitches; a rounded corner's radius grows by the
    overhang. An overhang under a first tier steeper than 75° at the shaft
    on any side is refused (`tower_overhang`, the reason in the message):
    a dome or ogee starting near vertical would hang its eave metres down
    the shaft (0.3 m put a dome's eave 6.3 m below the base); such a tower
    gets its eave from a short flatter first tier. 75° is Tectora's own
    bound [no source]: steeper than the steepest pitch a roofer lays an
    eave course at, shallow enough that the drop stays under 3.8 times the
    overhang. An eave under the roof around the tower is the warning
    `tower_eave_below_roof`; a building height that would put the eave
    under the ground is refused by `edit::height`.
  - The arcs keep to what the roof core takes: walls of at least 0.15 m
    and corners turning at least 2° (DACAD's own steps are 11.25° and 15°;
    finer arcs lost the roof or were calculated off); an arc touching
    another wall is refused. Every refusal ends with a stable code.
  - The outline keeps no arc: an arc is found again by its shape
    (`edit::arcs::arc_run`). Straightened, a rounded corner (an arc meeting
    both walls beside it tangentially, those walls running on to meet)
    becomes the sharp corner again; any other arc becomes its chord with
    the definition of its first wall, so a gable bent into an apse comes
    back as the eave the bend copied (audit r3 UX-004: telling the two
    apart would need the gable kept in the model; not done).
  - A tower is deleted with the building it stands in, rises with it, and
    a tower sunk into the roof is a warning (`tower_base_below_roof`).
- Follow-ups: a tower's shaft cladding and lantern walls in the price-sheet
  list if TERRAN wants them there (Jakub's choice); per-tier plan shapes;
  an eyebrow dormer.

## Sources

- DACAD's Runddach step rule: `crates/tectora-core/src/edit/round.rs`, help
  file topic Runddach (decoded, not in the repository).
- Chimney deduction and abutment lines: `docs/dacad-penetration-rules` in
  team memory, `openings.rs`.
- Area of a frustum band: standard surface-of-revolution / ruled-surface
  formula [no source — standard geometry, checked by the triangulation test].
