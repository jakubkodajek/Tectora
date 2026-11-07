# ADR-0002: Rust core with a Tauri 2 + Svelte desktop app

- **Status:** Accepted (recorded retroactively on 2026-11-07; in effect since 2026-10-26)
- **Date:** 2026-10-26
- **Deciders:** Jakub Kodajek

## Context

The first Tectora (June 2026, branch `feat/faze1-reconcile`) was a Python
app: PySide6 GUI, PyVista 3D, its own straight-skeleton roofs, a CLI and an
SQLite library. On 2026-10-26 work restarted as "Tectora 2" in a Rust
workspace with a Tauri 2 shell and a Svelte UI (commits e0f665f, f1d5471,
4e6c7bb); the Python app was removed from `main` on 2026-11-03 (6003380).

The requirements that the repository states for the product (`AGENTS.md`):
macOS and Windows as first-class platforms with native installers, one
roof engine shared by the app, the CLI and an MCP server, results identical
to DACAD, and AI agents editing the same jobs the user has open.

The reasons for the switch were not written down at the time; this ADR
records the decision as it shows in the code. Jakub can correct the context.

## Options considered

1. **Keep the Python app** - existing code and tests; but the roof logic was
   not derived from DACAD (now forbidden as a source of rules), and shipping
   PySide6 + PyVista installers on two platforms is heavy.
2. **Rust core + Tauri 2 shell + Svelte UI** - one compiled, deterministic
   core used by the app, the CLI and the MCP server; small native installers
   from CI; web UI with three.js for 3D. Cost: a full rewrite.

## Decision

Option 2, as built: `crates/tectora-core` holds all roof logic, `tectora-mcp`
and `tectora-cli` call it, `app/src-tauri` exposes it to a Svelte 5 UI.

## Consequences

- Positive: the same numbers everywhere; MCP parity is a matter of calling
  the same `edit` operation; CI builds `.dmg` and `.msi`/`.exe`
  (`.github/workflows/desktop.yml`).
- Negative / risks: the Tauri crate needs WebKitGTK to build on Linux (cloud
  containers exclude it); every Tauri command must also be mirrored in
  `examples/ui_bridge.rs` for the browser preview.
- Follow-ups: none.

## Sources

- Commits e0f665f, f1d5471, 4e6c7bb (2026-10-26), 6003380 (2026-11-03).
- Tauri 2 documentation: https://v2.tauri.app/
