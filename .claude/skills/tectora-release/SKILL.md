---
name: tectora-release
description: >-
  Ships a Tectora version to TERRAN: a preflight gate (tests, DACAD parity,
  i18n sweep, no proprietary files), version bump in all manifests, an
  annotated v* tag whose Czech message becomes the release notes, and the
  Release workflow that publishes signed installers and the in-app update
  (docs/releasing.md). Use when the user says "release",
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

- [ ] the Build workflow (`.github/workflows/build.yml`) has `check` and
      `windows-tests` green on the release commit (the Release workflow
      refuses it otherwise)
- [ ] the macOS .dmg installs and opens on Apple Silicon and Intel
- [ ] the Windows installer installs, `.tectora` opens by double-click
- [ ] a job from the previous release opens and calculates the same result
      (stored result shows no difference)
- [ ] the MCP server starts from the installed app (`<app> mcp`) and the
      "Connect AI" dialog shows a working command
- [ ] no open S1 or S2 bugs (`tectora-test`)

Any failure stops the release. Say which and why.

## Step 2: version

Same version in `app/src-tauri/tauri.conf.json` (what the installed apps
compare), `[workspace.package]` in `Cargo.toml` and `app/package.json`; the
test `every_manifest_declares_the_same_version` fails otherwise. Run
`cargo check` so `Cargo.lock` follows. Semver: a change of roof results or
of the file format is at least a minor bump and is named in the notes. The
bump goes to `main` through a PR like any change.

## Step 3: tag

After the user's go, and once `check` and `windows-tests` passed on that
commit of main:

```bash
git fetch origin main
git tag -a v0.X.Y -m "<Czech title>" -m "<Czech notes, - bullets>" origin/main
git push origin v0.X.Y
```

The tag's message is the release notes the users read in the app before
installing, so write it in Czech, short: what is new, what changed in
results (worded without naming DACAD), known limits.

`.github/workflows/release.yml` then builds the Windows NSIS installer and
the macOS universal app and .dmg, signs the updates with the release key,
checks them, writes `latest.json` and publishes the release to the public
releases repository named in `plugins.updater.endpoints`. Installed apps
offer it the next time the job library opens. If a check refuses the tag,
fix the cause, delete the tag (`git push origin :v0.X.Y`, only with the
user's go) and tag again.

The installers are not code-signed: macOS needs Privacy & Security > Open
Anyway the first time, and Windows SmartScreen asks once. Say so to new
users until signing is set up.

## Step 4: tell the users

First installs: `https://github.com/jakubkodajek/tectora-releases/releases/latest/download/Tectora_x64-setup.exe`
(Windows) and `.../Tectora_universal.dmg` (macOS). Existing installs update
themselves from the library.

## Output

The preflight results, the tag, the Release run and the published release
link, and the Czech release notes.
