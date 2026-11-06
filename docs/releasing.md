# Releasing

An installed app updates itself. When the job library opens, it asks
whether a newer version is out and offers to install it (`UpdateBanner`,
`app/src-tauri/src/updates.rs`). It is offered only in the library, never
in the editor, so no unsaved change is lost when installing ends the app.
Versions are published as GitHub releases: each carries the installers and
a `latest.json` that names the version, its notes and, for each platform,
the file and its signature. The app reads the address in
`plugins.updater.endpoints` in `app/src-tauri/tauri.conf.json`:

    https://github.com/jakubkodajek/Tectora/releases/latest/download/latest.json

which always points at the release marked latest; drafts and pre-releases
are never offered.

The releases are built by `.github/workflows/release.yml` when a version tag
is pushed. The repository is public, so its releases are too: anyone who
finds them can download the installers.

## Publish a new version

1. Raise the version (`0.1.0` to `0.2.0`, say) in all three places, which a
   test keeps equal (`every_manifest_declares_the_same_version`):
   `app/src-tauri/tauri.conf.json` (the one the app compares),
   `[workspace.package]` in `Cargo.toml`, and `app/package.json`. Then run
   `cargo check` so `Cargo.lock` follows, and merge it to `main`. The app
   offers only a version higher than its own. Wait until the Build
   workflow's `check` and `windows-tests` have passed on that commit.
2. Tag that commit of main with the version, with what changed as the tag's
   message, in Czech: the users at TERRAN read it before installing.

   ```sh
   git fetch origin main
   git tag -a v0.2.0 -m "Opravy výpočtu vikýřů" -m "- Rychlejší start" origin/main
   git push origin v0.2.0
   ```

   Make the tag with git, not on GitHub's Releases page: the workflow makes
   the release itself. A Claude session tags only after you say so.
3. Approve the run: GitHub mails you that the Release workflow waits for
   the `release` environment. In the run (Actions > Release), Review
   deployments > `release` > Approve and deploy. Approve only a run of a
   tag you pushed or asked for: the approval is what lets a run use the
   release key.
4. The Release workflow refuses a tag that does not name the version the
   commit declares, a commit that is not on main or whose checks have not
   passed, an empty public key, and a version that is not higher than every
   release. It builds the Windows installer and the macOS app into a draft
   release, then checks each update the way the installed apps will: signed
   by the key whose public half is in that commit's `tauri.conf.json`, for
   the version it declares. Only then does it write `latest.json` and
   publish the release. From then on each app offers it the next time the
   library opens. A run that failed can be run again from the Actions tab
   ("Re-run all jobs"); it starts from a fresh draft.

The latest installers for a first install always have the same address:
`https://github.com/jakubkodajek/Tectora/releases/latest/download/Tectora_x64-setup.exe`
for Windows and `.../Tectora_universal.dmg` for macOS (Apple silicon and
Intel).

Test installers of any branch, unsigned and without updates, come from the
Build workflow: Actions > Build > Run workflow, then the run's Artifacts.

## What the app checks

- The download is signed with the release key: the app verifies it against
  the public key in `plugins.updater.pubkey` and installs nothing that does
  not match ([Tauri docs: Updater](https://v2.tauri.app/plugin/updater/)).
- The signature also names the version it was signed for, and it must be
  the version `latest.json` announces (`requireSignedVersion`), so an old
  release cannot be passed off as a new one.
- The update server is reached over HTTPS only.
- On Windows the installer runs in its passive mode (a progress bar, no
  questions) and starts the new version; on macOS the app replaces itself
  and starts again.
- Debug builds (`npx tauri dev`, the browser preview) never ask the server.
  The preview shows the offer with `TECTORA_PREVIEW_UPDATE=0.2.0 npm run preview`.

## The AI tools

The MCP server is the app itself (`<app> mcp`), which a connected AI app
starts. After an update the AI app keeps the old version's server running
until it is closed and opened again.

A release build merges `app/src-tauri/tauri.release.conf.json` into the
configuration, which makes the bundler sign the updater files. Every other
build (`npx tauri build`, the test installers of `build.yml`) needs no key.

The jobs that hold the release key run no action from another repository,
only the runner's own tools and the app's locked dependencies, installed
without their install scripts: whoever could change such an action could
take the key and sign an update every installed app accepts.

## Code signing

The release key signs updates only. The installers themselves are not
code-signed: a first install shows the operating system's warning once. On
Windows, SmartScreen's "Windows protected your PC" > More info > Run anyway;
on macOS the app is ad-hoc signed, so Apple silicon runs it, but Gatekeeper
blocks its first start until System Settings > Privacy & Security > Open
Anyway. Signing with an Apple Developer ID (and notarization) or a Windows
code-signing certificate would need the certificate's secrets and the
bundler's signing settings
([Tauri docs: macOS](https://v2.tauri.app/distribute/sign/macos/),
[Windows](https://v2.tauri.app/distribute/sign/windows/)).

## One-time setup

1. **Release key.** On your own computer, in `app/`:
   `npx tauri signer generate -w ~/.tauri/tectora.key`, with a password.
   Keep the key file and its password safe and private: without them no
   installed app can be updated again, and anyone who has them can sign an
   update the apps accept. Put the public key (the content of
   `~/.tauri/tectora.key.pub`) in `plugins.updater.pubkey` in
   `app/src-tauri/tauri.conf.json`. Use a key of its own, not another
   app's.
2. **An environment for the key.** In this repository's Settings >
   Environments, create `release`, and under Deployment branches and tags
   choose "Selected branches and tags" and add the tag rule `v*`. Under
   Required reviewers add yourself, and leave "Prevent self-reviews" off,
   or you cannot approve your own releases. Clear "Allow administrators to
   bypass configured protection rules": sessions that act as you are admins
   too. Every run that would use the key then waits for your approval, and
   GitHub mails you about each one. Both are free on a public repository
   ([GitHub docs: Deployments and environments](https://docs.github.com/en/actions/reference/workflows-and-actions/deployments-and-environments)).
3. **Secrets** of that environment: `TAURI_SIGNING_PRIVATE_KEY` (the content
   of `~/.tauri/tectora.key`) and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.

A changed key needs a manual reinstall on every computer, since the apps
there only accept the old one.
