# ADR-0008: Bug reports are sent through a relay the owner runs

- **Status:** Proposed
- **Date:** 2026-11-28
- **Deciders:** Jakub Kodajek

## Context

The first version of "Report a bug" wrote a ZIP into a reports folder each
user had to set in Settings (a shared Drive or iCloud folder), or to a place
chosen with Save as. Several colleagues will use the app and none of them
should configure anything: one click sends the report, the owner triages
later. The owner wants the reports in his Google Drive and as issues in his
private GitHub repository.

Forces:

- The app must not carry a credential that gives access to the repository or
  to the Drive (installers can be unpacked).
- Offline first: the app works without a network, and a report made offline
  must not be lost.
- MCP parity: an AI client must be able to send a report too, also when it
  runs the standalone `tectora mcp`.
- `tectora-core` has no network code and no TLS stack today. The only HTTP
  client in the workspace is `reqwest`, pulled in by `tauri-plugin-updater`
  for `tectora-app`.
- Tests must run on Linux without WebKitGTK, where `tectora-app` is excluded.

## Options considered

1. **Keep the shared folder** - no network code, no service; but every user
   has to install a sync client and set a folder, which is what the owner
   rejects.
2. **GitHub API straight from the app** - one hop; but the token would be in
   every installer, issues cannot hold a ZIP of this size, and colleagues have
   no GitHub accounts.
3. **A relay as a Google Apps Script web app under the owner's account**
   (chosen) - free, no server to run, runs as the owner so it can write to his
   Drive and hold the GitHub token in Script Properties; the app holds only
   the endpoint and a shared key. Cons: request-size and quota limits of Apps
   Script, answers always come as HTTP 200 behind a 302 redirect, the shared
   key is extractable.
4. **A small hosted service (Cloudflare Worker, Cloud Run)** - cleaner HTTP,
   real status codes, larger bodies; but an account, billing and a deployment
   to maintain for a handful of reports a month.
5. **E-mail the report** - nothing to deploy; but needs an SMTP credential in
   the app or the user's mail client, and attachments of this size bounce.

Where the HTTP code lives:

- a. In `app/src-tauri` only - no new crate; but untestable on Linux CI, the
  standalone MCP server cannot send, the preview bridge cannot exercise it.
- b. In `tectora-core` behind a feature - one crate fewer; but the core gains
  a TLS stack and an async runtime in its dependency graph.
- c. **A new crate `crates/tectora-relay`** (chosen) - HTTP only, no
  dependency on the core; used by `tectora-mcp` (and through it the app and
  the CLI) and by the preview bridge as a dev-dependency of the core.

## Decision

We choose **option 3 with crate layout c** because it meets "nothing to
configure" without putting any repository or Drive credential into the app
and without a service to operate.

- The app and the MCP server put every report into an outbox under the
  configuration folder (`tectora-core::outbox`, std only) and send it with
  `tectora-relay::send`: one JSON `POST` with the ZIP base64-encoded, the
  shared key and a small allow-listed `meta` object for the issue. Failed
  sends stay queued and are retried with backoff.
- The relay (`integrations/report-relay/Code.gs`) checks the key, size and
  rate limits, stores the ZIP in the owner's Drive folder and opens the issue
  from `meta`. It is idempotent by report id.
- The endpoint URL and the key are compiled in with `option_env!` from the
  GitHub Actions secrets `TECTORA_REPORT_URL` and `TECTORA_REPORT_KEY`.
  Builds without them cannot send and offer Save as only.
- The `reports_dir` setting is removed (it never shipped).
- `reqwest` and `base64` become direct dependencies of the new crate; both
  are already in `Cargo.lock`, and the change must not add a third-party
  package.

## Consequences

- Positive: colleagues configure nothing; reports arrive as issues with a
  Drive link; the token stays in the owner's script; the core stays free of
  network code; the queue and the client are testable separately (fake sender,
  local mock relay with the 302 flow).
- Negative / risks:
  - The shared key is not a secret once an installer is public. Abuse is
    bounded by the script's day limits and byte budget; rotating the key
    needs a release, and older installs then queue reports until updated.
  - Apps Script limits (request size, run time, daily quotas) are outside our
    control; the upload cap (20 MB planned) must be confirmed with a probe.
  - The app now makes an outbound request to a second host (besides the
    update check). It happens only after the user presses Send, or to retry
    such a report.
  - A new workspace crate, and a Google-side script that is deployed by hand
    and versioned in the repository.
  - Customer data leaves the computer inside the ZIP (to the owner's Drive).
    The issue text is built from an allow-list that excludes customer name
    and address.
- Follow-ups: `docs/bug-reports.md` (owner setup), the two repository
  secrets, a note in `docs/releasing.md`, the architecture data flow, and a
  decision on the open questions in
  `target/work/bug-report/plan-delivery.md` (job name in the issue, who
  reported, release without secrets).

## Sources

- Apps Script web apps: `doPost(e)` event object (`e.postData.contents`, no
  request headers), `ContentService` answers served through a redirect to
  `script.googleusercontent.com`:
  https://developers.google.com/apps-script/guides/web and
  https://developers.google.com/apps-script/guides/content
- Apps Script quotas: https://developers.google.com/apps-script/guides/services/quotas
- GitHub fine-grained personal access tokens and the Issues permission:
  https://docs.github.com/en/rest/issues/issues#create-an-issue
- `Cargo.lock`: `reqwest 0.13.5` (through `tauri-plugin-updater 2.14.1`),
  `base64 0.23.1`.
- The request-size limit for web app `POST` bodies is not confirmed by these
  sources; the plan measures it (probe, step 9).
