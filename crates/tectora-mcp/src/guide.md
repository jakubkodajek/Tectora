# Tectora for AI agents

Tectora is a roof design program modelled on DACAD (COIN GmbH). The user has
the Tectora desktop app open. Every change you save appears there immediately
(hot reload): the app highlights what you changed, shows your one-sentence
`summary`, and lets the user accept or revert it. Write summaries for a roofer,
in the user's language, e.g. "Pitch of the street side set to 42° per drawing".

## Model

- Units: metres and degrees. Plan coordinates: x to the right, y up (north).
- A project has one or more **parts** (roof bodies). A part is a closed
  footprint of **wall lines** (the outer face of the walls, *not* the eave
  overhang). Points are listed in order; edge `i` runs from point `i` to
  point `i+1` (the last edge closes back to point 0). The order may be
  clockwise or counter-clockwise; Tectora stores it counter-clockwise.
- Like in DACAD, **every edge carries its own roof definition**:
  - `pitch_deg` – roof pitch rising from this edge (1–89°).
  - `gable: true` – gable wall (Ortgang / štít): no roof plane rises from it
    (stored as pitch 90°).
  - `eave_height` – height of the eave line above ±0.00, m, measured at the
    outer (overhang) line as in DACAD.
  - `overhang` – horizontal eave overhang beyond the wall, m.
  - `half_hip` (gable edges only) – Krüppelwalm / polovalba:
    `{ "height": m above ±0.00 where the hip starts, "pitch_deg": … }`.
  - `priority` (0 or 1) – where this plane meets a parallel plane with a
    different pitch (e.g. a recessed or projecting wall section), the plane
    with priority 1 ends at their horizontal line; without priority the
    steeper one ends, as in DACAD.
  - `breaks` – pitch breaks (Knick / zlom, e.g. a mansard roof):
    `[{ "height": m above ±0.00, "pitch_deg": … }]`; from each height on the
    plane continues at the new pitch.
- Typical values: pitch 25–45° (tiles), 5–15° (sheet metal / shed roofs),
  eave height 3–7 m, overhang 0.3–0.8 m.
- Roof types: gable roof = eaves on the long sides + gables on the short
  sides; hip roof = eaves everywhere; shed (Pultdach) = one eave + three
  gables; pyramid = hip roof on a square; half-hip = gable roof whose gables
  have `half_hip`.

## Results (DACAD line codes)

`calc` returns the roof area and line lengths keyed by DACAD codes:
T eave (Traufe/okap), F ridge (First/hřeben), G hip (Grat/nároží),
K valley (Kehle/úžlabí), OL/OR/OS verge left/right/oblique (Ortgang/štítová hrana),
P top edge of a shed roof against its wall (Pult), c pitch break (Knick/zlom).
The rules were checked against roofs calculated by DACAD itself: a step
between eaves at different heights is a short verge, the lower edge of a
Krüppelwalm plane counts as eave T, and hips run through a pitch break.

## Turning a technical drawing into a roof

1. Read the drawing: footprint dimensions, which sides are eaves and which
   are gables (look at the elevations / sections), pitches, eave height,
   overhang. Note the drawing's scale and units.
2. Put the footprint in metres with one corner at (0, 0). For a building made
   of several wings with different ridge heights, use one part per wing.
3. Use `create_standard_roof` for a plain rectangle, otherwise
   `create_project` with the points and per-edge definitions.
4. Call `calc` and sanity-check the area and ridge height against the drawing.
5. Fix individual values with `update_edges`, `set_outline` or `update_job`.
   Each save is one reviewable step, so prefer a few meaningful steps over
   many tiny ones, and always explain them in `summary`.
6. Tell the user what you assumed (e.g. "overhang not dimensioned, used 0.5 m").

Never invent dimensions you cannot read; ask the user instead.
