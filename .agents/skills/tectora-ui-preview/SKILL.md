---
name: tectora-ui-preview
description: >-
  Runs the Tectora UI in a plain browser against the real Rust core (no Tauri,
  works in the cloud too), takes screenshots across window sizes, languages and
  themes with automatic layout checks, and shows MCP edits live. Use when the
  user says "show me the UI", "screenshot", "how does it look", "check the
  layout", "does it fit in a small window", "test it in German", "before and
  after", or after any change in app/src.
---

# tectora-ui-preview

## How it works

- `cd app && npm run preview` (or the `ui-preview` entry in
  `.claude/launch.json`) builds and starts two things:
  - `ui_bridge` (`crates/tectora-core/examples/ui_bridge.rs`): an HTTP server
    on 127.0.0.1:7878 that answers the same commands as
    `app/src-tauri/src/lib.rs`, over a real library in
    `target/ui-preview-library` (seeded with example roofs on first start).
  - Vite on http://localhost:1430, where `/` serves `app/preview.html`. It
    loads `app/src/dev/tauri-mock.ts`, which sends every `invoke` to the bridge
    and turns file changes into the app's usual events.
- The `tectora` MCP server in `.mcp.json` uses the same folder. Any MCP write
  (from this session or another AI client pointed at that folder) appears in
  the open preview within a second: banner, orange highlight, Accept / Revert.
- Not covered: native file dialogs (a prompt asks for a path), opening
  attachments, double-click file associations, window chrome. Test those in
  `npx tauri dev` on macOS and Windows.

Other folder or ports: `TECTORA_PREVIEW_DIR=/path TECTORA_PREVIEW_PORT=1431 npm run preview`,
and `/preview.html?bridge=http://127.0.0.1:7879` for a second bridge.

## Screenshots

With the preview running:

```bash
node .claude/skills/tectora-ui-preview/shoot.mjs \
  --screens home,editor --sizes 1360x860,960x540,600x420 \
  --locales cs,sk,en,de --themes light,dark --out target/ui-shots
```

`--click "<selector>"` (repeatable) clicks further after the screen opens, for
example a toolbar tool or a dock tab. Prefer role or text selectors
(`"text=Výpočet"`, `"role=tab[name='AI']"`) over generated Svelte classes.

Each shot prints what it found: page scroll, elements outside the window,
truncated buttons, unexpected horizontal scrollbars, page errors and console
errors. Exit code 1 if anything was found. Look at the images too: the checks
do not see overlaps, wrong colours or unreadable plan labels.

Needs Playwright with Chromium (`npm i -g playwright && npx playwright install chromium`
locally; preinstalled in the cloud container).

## Window sizes that matter

Desktop windows, not phones: 1440x900 and 1360x860 (laptops), 1920x1080,
960x1040 (half of a full-HD screen), 1440x450 (low window), and the minimum
600x420. German is the longest language; test it at the narrow sizes.

## Before and after

For a visual change, shoot the same matrix on the base branch (a `git worktree`
with its own preview on another port and bridge) and on the change, then put
pairs side by side in the PR or the reply.

## When the preview breaks

- `ui_bridge: unknown command X` in the browser console: a Tauri command was
  added to `app/src-tauri/src/lib.rs` but not to `ui_bridge.rs`. Add it there,
  with the same arguments (JS sends camelCase keys).
- Blank page: open the console; a page error there is a real UI bug unless it
  comes from `tauri-mock.ts`.

## Output

Screenshots in `target/ui-shots/` (gitignored), the findings list, and for a
change the before / after pairs.
