---
name: tectora-release
description: >-
  Ships a Tectora version to TERRAN: a preflight gate (tests, DACAD parity,
  i18n sweep, no proprietary files), version bump in all manifests, a v* tag
  that makes CI build the macOS universal .dmg and the Windows installers, and
  release notes in Czech for the users. Use when the user says "release",
  "new version", "build the installer", "ship it", "send it to the
  colleagues", or "tag v0.x".
---

# tectora-release

## The rules

- **Nothing is tagged without the user's go.** Show the preflight and ask.
- **Old jobs must open.** A release never breaks a `.tectora` or `.tectorajob`
  file from an earlier version: schema changes come with a migration and a test.
- **No proprietary material in the build or the repo**: no `.DAD`, DACAD
  binaries or decompiled code, nothing with the DACAD name in the UI.

## Step 1: preflight

```bash
git status --short                      # clean tree
git ls-files | grep -iE '\.(dad|exe|dll|hlp)$' && echo "STOP: proprietary file tracked"
cargo fmt --all --check && cargo clippy --workspace --all-targets
cargo test --workspace
PROPTEST_CASES=20000 cargo test --release -p tectora-core --test properties
cd app && npm ci && npm run check && npm test && npm run build && cd ..
python3 .claude/skills/tectora-i18n/sweep.py
python3 .claude/skills/tectora-design/contrast.py
```

Plus by hand, or as a checklist for the user:

- [ ] the CI run of `.github/workflows/desktop.yml` on the release commit is
      green on Linux, macOS and Windows
- [ ] the macOS .dmg installs and opens on Apple Silicon and Intel
- [ ] the Windows installer installs, `.tectora` opens by double-click
- [ ] a job from the previous release opens and calculates the same result
      (stored result shows no difference)
- [ ] the MCP server starts from the installed app (`<app> mcp`) and the
      "Connect AI" dialog shows a working command
- [ ] no open S1 or S2 bugs (`tectora-test`)

Any failure stops the release. Say which and why.

## Step 2: version

Same version in `Cargo.toml` (`[workspace.package]`), every crate that pins
its own, `app/src-tauri/tauri.conf.json` and `app/package.json`. Semver: a
change of roof results or of the file format is at least a minor bump and is
named in the notes.

## Step 3: tag and build

After the user's go, on the release commit:

```bash
git tag -a v0.X.Y -m "Tectora 0.X.Y"
git push origin v0.X.Y
```

The `v*` tag runs the desktop workflow: `Tectora-macOS` (universal .dmg) and
`Tectora-Windows` (.msi and NSIS .exe) artifacts on the run. The installers
are not code-signed yet: macOS needs right-click > Open the first time, and
Windows SmartScreen asks once. Say so in the notes until signing is set up.

## Step 4: release notes

For the colleagues at TERRAN, in Czech, short: what is new, what changed in
results (and why: "now matches DACAD for …", worded without naming DACAD if the
notes reach customers), how to install, known limits. Engineering notes in
English go in the GitHub release.

## Output

The preflight results, the tag, the CI run link with the installers, and the
Czech release notes ready to send.
