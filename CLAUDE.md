# Tectora

A desktop roof design app for TERRAN (a roofing company) that replaces DACAD.
The user draws the walls of a building, defines the roof on each wall (eave or
gable, pitch, eave height, overhang, half-hip, pitch breaks, …) and gets the
roof, its area and its line lengths. DACAD is the reference: the roof core is a
1:1 port of DACAD's own core, checked against the files DACAD itself writes.
Tectora is its own product and is not affiliated with DACAD or COIN GmbH.

## Product shape

- One desktop app for **macOS and Windows** (Tauri 2). Both platforms are
  first-class; never use an API, path or shortcut that works on only one.
- **Job library** (zakázky): every saved roof is a job with customer, address,
  status (draft, offer, ordered, done), attached drawings and a change log.
- **Editor**: DACAD-style, simpler and modern. Plan + 3D, roof definition bar
  per wall, standard roofs, dormers, chimneys, windows, openings, calculation
  helpers, results by DACAD line code (T, F, G, K, O, …) and by material.
- **AI**: the MCP server (`tectora mcp`) can do everything the UI can. The open
  app hot-reloads every AI save, highlights what changed and offers Accept /
  Revert. Goal: AI builds the roof from technical drawings and a colleague only
  reviews it (`ROADMAP.md`, section "AI").
- UI languages: Czech (default), Slovak, English, German.

## Stack

- Rust workspace (edition 2024). Tauri 2 shell in `app/src-tauri`. UI in
  Svelte 5 + TypeScript (Vite), 3D with three.js.
- All roof logic lives in `crates/tectora-core`; the UI and the MCP server call
  the same `tectora_core::edit` operations. No roof math in TypeScript.
- Project file: `.tectora` JSON with a versioned schema
  (`docs/tectora.schema.json`, migrations on load). Job bundle: `.tectorajob`.

## Commands

Run from the repo root unless noted.

| task | command |
| --- | --- |
| install UI deps | `cd app && npm ci` |
| run the app (dev) | `cd app && npx tauri dev` |
| UI in a browser, real core, no Tauri | `cd app && npm run preview`, open http://localhost:1430 (see below) |
| all Rust tests | `cargo test --workspace` (Linux without WebKitGTK, e.g. the cloud container: add `--exclude tectora-app`) |
| core tests only | `cargo test -p tectora-core` |
| lint / format Rust | `cargo clippy --workspace --all-targets` / `cargo fmt --all` |
| type-check UI | `cd app && npm run check` |
| UI unit tests | `cd app && npm test` |
| UI build | `cd app && npm run build` |
| calculate a file | `cargo run -q -p tectora-cli -- calc path/to/job.tectora --summary` |
| MCP server | `cargo run -q -p tectora-cli -- mcp [--dir <library>]` |
| update the file schema | `TECTORA_WRITE_SCHEMA=1 cargo test -p tectora-core --test schema` |
| installers | CI: `.github/workflows/desktop.yml` (macOS universal .dmg, Windows .msi/.exe); tag `v*` for a release build |

**Browser preview.** `npm run preview` builds and starts the `ui_bridge`
example (`crates/tectora-core/examples/ui_bridge.rs`), which answers the Tauri
commands over a real library in `target/ui-preview-library`, and Vite on port
1430 with a Tauri stand-in (`app/src/dev/tauri-mock.ts`). The project MCP
server in `.mcp.json` points at the same folder, so MCP edits show up live in
the preview. When you add a Tauri command in `app/src-tauri/src/lib.rs`, add it
to `ui_bridge.rs` too. Use the `tectora-ui-preview` skill for screenshots.

## Layout

- `crates/tectora-core`: model (`model.rs`), DACAD core port (`dcore/`),
  multi-roof join (`modell/`), SOL/INT/W3D writers (`coresol/`), dormers,
  openings, quantities, store (library, activity log, attachments), bundles,
  snapshots, editor operations (`edit/`).
- `crates/tectora-mcp`: MCP server; `src/guide.md` is what AI clients read first.
- `crates/tectora-cli`: the `tectora` binary.
- `app/src`: Svelte UI (`lib/`), translations in `lib/i18n.svelte.ts`, theme
  tokens in `app.css`.
- `tests/fixtures`: DACAD's own outputs the core is checked against.
- `integrations/skills/tectora-roof`: the skill end users give their AI client
  for drawing-to-roof. Keep it in step with the MCP tools.

## Where things live

- `ROADMAP.md`: what is done and what is next, against DACAD's command list.
- `README.md`: overview and build. `docs/ai-integration.md`: MCP tools and the
  live-review flow. `docs/design-brief.md` and `docs/design/`: UI direction.
- `.claude/skills/tectora-*`: workflows for this repo (DACAD parity, UI preview,
  design, i18n, testing, release). Use them.
- `.claude/agents/tectora-*-reviewer.md`: four reviewer subagents (UX, UI
  controls, roof math, architecture and security) with clean context and a
  shared findings board; run them with the `tectora-review` skill.

## Rules

- **DACAD is the ground truth for roof logic.** Derive every rule from DACAD
  (its outputs, help file, reverse engineering), never from the old Python
  Tectora or from "what looks geometrically right". Stay byte-exact with DACAD
  even where DACAD is geometrically odd (hips outside the outline etc.); show a
  localized warning for suspicious roofs instead of changing the result.
- **No DACAD in the product.** The UI never shows the name DACAD, COIN or any
  original German DACAD string (e.g. raw `E00002 Nicht geschlossene …`). DACAD
  terms belong only in code comments, docs and tests. Every message is
  localized in all four languages.
- **No DACAD import or export.** No `.DAD` reading or writing in the app, CLI or
  MCP. The `.DAD` reader exists only for dev tools in `examples/support/`.
- **Never commit DACAD material**: `.DAD` files, `DACAD*.exe`, its DLLs, licence
  keys, decompiled code or its help file. `.gitignore` covers the common ones;
  check `git status` before every commit. Tests that need them read paths from
  env vars (`DACAD_SAMPLES`, …) and are skipped or ignored without them.
- **MCP parity.** Every editor operation has an MCP tool, and every MCP write
  needs a one-sentence `summary` for the user. A UI feature without its MCP tool
  is not done.
- **Error and notification UI is minimal and technical**: small status mark,
  neutral surface, monospace code and details. No tinted panels, big icons or
  coloured side bars.
- Branding follows TERRAN: brick red `#a8362c`, orange `#db8839` (AI), grey
  `#5c5b5a`. Colours only through the tokens in `app/src/app.css`.
- `window.confirm` is async under Tauri's dialog plugin: always `await` it.
- Treat job files, bundles and MCP input as untrusted: validate ids, sizes and
  coordinate ranges before calculating or writing, and never let a bad roof
  panic or hang the app.
- English for all code, comments, commit messages, docs and identifiers.
