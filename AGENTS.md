# Tectora

Canonical instructions for every AI coding agent in this repository (Claude
Code, OpenAI Codex, GitHub Copilot, Cursor, Gemini, …). `CLAUDE.md` imports
this file; `.github/copilot-instructions.md` points here. Every line is loaded
into every session, so keep it short and move procedures into skills.

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

Run from the repo root unless noted. `make` wraps the gate that agents must
run and show the output of before they call anything done:

| task | command |
| --- | --- |
| everything CI checks (agent files, fmt, clippy, tests, UI check/test/build) | `make check` |
| i18n and colour-contrast sweeps (UI changes) | `make sweeps` |
| install UI deps | `make setup` |
| lint only / tests only | `make lint` / `make test` |
| UI end-to-end tests (when `app/e2e` exists) | `make e2e` |

`make` excludes `tectora-app` from cargo automatically when WebKitGTK is
missing (Linux cloud containers). The underlying commands:

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
| test installers | Actions > Build > Run workflow (`.github/workflows/build.yml`): macOS universal .dmg, Windows NSIS .exe |
| release | annotated tag `v<version>` on main, only on the user's go: `.github/workflows/release.yml` publishes signed installers and the in-app update (`docs/releasing.md`) |

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
- `.claude/`: agents, skills, path rules and hooks (the source for all AI
  tools); `.codex/`, `.github/agents/`, `.agents/` are generated from it by
  `scripts/sync_agents.py`. Scratch work goes to `target/work/` and
  `target/review/` (git-ignored).

## Where things live

- `ROADMAP.md`: what is done and what is next, against DACAD's command list.
- `README.md`: overview and build. `docs/ai-integration.md`: MCP tools and the
  live-review flow. `docs/design-brief.md` and `docs/design/`: UI direction.
- `.claude/skills/tectora-*`: workflows for this repo (DACAD parity, UI preview,
  design, i18n, testing, release). Use them.
- `.claude/agents/tectora-*`: six reviewer subagents (UX, UI controls, roof
  math, architecture and security, tests, silent failures) and a finding
  verifier, each with clean context and a shared findings board; run them
  with the `tectora-review` skill.
- `.claude/agents/tectora-{planner,test-engineer,implementer,bug-hunter,docs-keeper}`:
  the build pipeline, run by the `tectora-feature` skill
  (`docs/agent-workflow.md`).
- `docs/architecture.md`: components, the calculation pipeline and the data
  flow between app, core, MCP and the job library. `docs/adr/`: decisions.
- `memory/`: durable project memory for humans and agents (see
  `memory/README.md`). `REVIEW.md`: what reviewers flag and how severe it is.

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

## Working rules for agents

1. **Explore, plan, implement, verify, review.** Read the code a change
   touches before writing any. Anything bigger than a small fix gets a plan in
   `target/work/<slug>/plan.md` first (`tectora-feature` skill).
2. **One feature per session.** Take one item from `memory/features.json`,
   finish it, verify it, commit it.
3. **Evidence over assertion.** Done means `make check` passed and you showed
   the output; UI work also needs a preview screenshot; roof logic needs a
   DACAD fixture or closed-form test.
4. **Never weaken tests.** Do not delete, skip, `#[ignore]` or loosen a test or
   fixture without a human decision written in the PR. A changed DACAD fixture
   is a human decision, always.
5. **Separation of duties.** The agent that wrote the code does not approve
   it: reviews run in fresh contexts (`tectora-review`), and only a human
   merges.
6. **Small commits**, imperative subject line saying what the change does for
   the user (the style of `git log`), one topic per commit.
7. **Update memory before you stop** (`tectora-handoff` skill): a line in
   `memory/progress.md`, new decisions in `memory/decisions.md` (or an ADR),
   repeated mistakes in `memory/lessons-learned.md`.

