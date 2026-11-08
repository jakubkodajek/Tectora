---
name: tectora-design
description: >-
  Keeps the Tectora UI on its design system: TERRAN brand colours as theme
  tokens in app.css (light and dark), DACAD-like layout made simpler and
  modern, minimal technical error UI, and a WCAG contrast check of the tokens.
  Use when the user says "change the colours", "new component", "make it look
  better", "design this screen", "dark mode", "error looks bad", "AI slop",
  "check contrast", or when a change touches app/src/app.css or a component's
  styles.
---

# tectora-design

## Direction

- **Looks like DACAD, simpler and modern.** Users know DACAD: keep the plan in
  the middle, the roof definition bar for the selected wall, the line codes and
  the step-through of walls. Remove clutter, not function.
- **Two priorities on screen**: the job library (find a past roof fast) and the
  AI review (show exactly what the AI changed, with Accept / Revert).
- **TERRAN brand**: brick red `--accent` (#a8362c), orange `--ai` (#db8839) for
  everything the AI did, grey #5c5b5a in the logo. Orange means "AI", nothing else.
- **Errors and notices are minimal and technical**: a small status square,
  neutral surface, the message in one line, code and details in monospace.
  No tinted panels, no big icons, no coloured side bars, no emoji, no
  gradients. Never show a raw core message or code; map it to an i18n key.
- Plan line colours (`--line-*`) are fixed meanings shared with the legend and
  the 3D view. A new line type gets a new token, not a reused one.

## Tokens

All colours, radii and shadows are CSS custom properties in `app/src/app.css`:
a light `:root` block and a dark block (system theme, or `data-theme` when the
app sets it). Components use `var(--…)` only. A raw hex value in a component
is a bug, except in the logo and the app icon.

New token: add it to the light and the dark block, then run the contrast check.

## Contrast check

```bash
python3 .claude/skills/tectora-design/contrast.py            # app/src/app.css, every theme
python3 .claude/skills/tectora-design/contrast.py "#db8839" "#ffffff"
```

Text needs 4.5:1, large text and UI parts (plan lines, borders, focus ring)
3:1 (WCAG 2.2, https://www.w3.org/TR/WCAG22/#contrast-minimum). It exits 1 on
a failure. Fix the token, not the component. When you add a token that is
drawn as text or a line, add its pair to `PAIRS` in `contrast.py`.

## Components

Before writing a new component, look for one that already does the job
(`Modal`, `Icon`, the bars in `app/src/lib/`). For a new one, decide:

- states: default, hover, active, focus-visible (2px ring, `--focus`),
  disabled, loading, error, empty
- keyboard: reachable with Tab, Enter / Space act, Escape closes
- text: every label from `t(...)`, room for German (about 30 % longer)
- the smallest window, 600x420: what collapses, what scrolls

Icons: the inline set in `Icon.svelte`, same stroke width. No icon fonts and
no remote assets (the app runs offline).

## Check it

Run the preview and shoot light and dark, cs and de, wide and narrow
(`tectora-ui-preview`). Attach before / after for any visible change.

## Output

Updated tokens and components, a clean contrast report, and screenshots.
