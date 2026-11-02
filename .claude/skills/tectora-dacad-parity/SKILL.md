---
name: tectora-dacad-parity
description: >-
  Adds or fixes a roof feature so Tectora gives exactly DACAD's result: find
  DACAD's behaviour from its outputs, help and reverse engineering, capture it
  as fixtures, port it into tectora-core, prove it byte-exact, then expose it
  in the editor and the MCP. Use when the user says "add <DACAD feature>",
  "port Trapezzerlegung / Bahnenteilung / Sparren ...", "the roof differs from
  DACAD", "match DACAD", "is this what DACAD does", or picks an item from the
  ROADMAP "Next" list.
---

# tectora-dacad-parity

DACAD is the ground truth. A feature is done when Tectora gives DACAD's numbers
for DACAD's inputs, the user can do it in the editor, an AI can do it over MCP,
and none of it says "DACAD" on screen.

## The rules

- **From DACAD, not from geometry or old code.** Rules come from DACAD's own
  outputs (`.DAD` results, captured core files `RD2/SO2/SOL/INT/W3D`), its help
  file and reverse engineering of `DACAD DE.exe`. Never from the old Python
  Tectora and never from "what would be correct". If DACAD is geometrically odd,
  copy it and add a localized warning in the UI instead.
- **Clean repository.** Never commit `.DAD` files, `DACAD*.exe`, DLLs, licence
  data, the help file or decompiled code. Fixtures in `tests/fixtures/` are
  DACAD's outputs for our own synthetic inputs, named by case (`C67_…`).
  Paths to private material come from env vars (`DACAD_SAMPLES`, …).
- **Write down what you learned.** A rule found by RE gets a comment at the
  code that implements it, naming the DACAD function or message it mirrors
  (for example `// DACAD FUN_00d9cd80: height sweep`). The next person must not
  have to rediscover it.

## Step 1: what DACAD does

1. Find the command in DACAD's menu (its string table and help file) and the
   ROADMAP line it belongs to.
2. List its inputs, its options and what it changes in the result: which line
   codes (T, F, G, K, O, P, S, W…), which areas, which errors.
3. Make the smallest inputs that isolate each rule: one case per rule, plus the
   edge cases (zero length, 90° pitch, touching parts, negative overhang).
4. Get DACAD's answer for each case (run DACAD, or the byte-exact emulator of
   its core if one is set up in the session). Mark every rule as **measured**
   (from a DACAD run) or **guess**. Guesses do not ship as parity.

## Step 2: fixtures first

Put DACAD's outputs for the new cases in `tests/fixtures/<area>/` and write the
failing test in `crates/tectora-core/tests/` that compares Tectora with them:
byte-exact for core files, within DACAD's own rounding (2 decimals) for
quantities. Watch it fail for the right reason.

## Step 3: port into the core

- All logic in `crates/tectora-core`. The core port in `dcore/` mirrors DACAD's
  C core: same tolerances (POLYG 0.02, VALUE 0.01, ANGLE 0.5°, POINT 0.02,
  INTSCT 0.05), same float32 rounding, same sweep order. Do not "improve" them.
- New model fields: add them to `model.rs` with serde defaults, bump the schema
  version with a migration if old files need one, regenerate
  `docs/tectora.schema.json` (`TECTORA_WRITE_SCHEMA=1 cargo test -p tectora-core --test schema`).
- Editor operations go in `edit/`, so the UI and the MCP share them.

## Step 4: prove it

```bash
cargo test -p tectora-core                      # fixtures, analytic, invariants
cargo test --workspace
PROPTEST_CASES=5000 cargo test --release -p tectora-core --test properties
```

Every old fixture must still pass. If an old case changes, stop: either the new
rule is wrong or the old fixture was. Find out which with a DACAD run.

## Step 5: expose it

- **UI**: the editor control, in the DACAD-like place but simpler. Texts in all
  four languages (`tectora-i18n`), no DACAD terms on screen.
- **MCP**: a tool or a parameter in `crates/tectora-mcp`, documented in
  `src/guide.md` and `docs/ai-integration.md`; every write takes a `summary`.
  Update `integrations/skills/tectora-roof/SKILL.md` if the drawing workflow
  changes.
- **Preview**: check it in the browser preview (`tectora-ui-preview`).

## Step 6: record it

- Move the item in `ROADMAP.md` from "Next" to "Done".
- PR description: the cases, measured vs guess, fixtures added, and any DACAD
  oddity you copied on purpose.

## Output

Fixtures, tests, the core change, the UI control, the MCP tool, ROADMAP
updated, and a summary: cases measured, cases passing, anything still a guess.
