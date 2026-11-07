---
paths:
  - "app/src/**/*.svelte"
  - "app/src/**/*.ts"
  - "app/src/app.css"
---

# Rules for the Svelte UI

- No roof math in TypeScript: call the core through `app/src/lib/api.ts`.
  A new Tauri command also goes into `crates/tectora-core/examples/ui_bridge.rs`
  so the browser preview keeps working.
- Every string goes through `app/src/lib/i18n.svelte.ts` in cs, sk, en and de.
  No DACAD, COIN or German DACAD term in any UI text. Run
  `python3 .claude/skills/tectora-i18n/sweep.py`.
- Colours only through the tokens in `app/src/app.css`; text contrast at
  least 4.5:1, UI lines 3:1 (`python3 .claude/skills/tectora-design/contrast.py`).
- Errors and notifications stay minimal and technical (status mark, neutral
  surface, monospace code), localized, never silent.
- `window.confirm` is async under Tauri's dialog plugin: always `await` it.
- Every control works by keyboard with a visible focus state; the layout
  holds at 600x420 and in half-screen windows.
- Check the change in the browser preview (`tectora-ui-preview`) and attach
  a screenshot to the report.
