# Bug reports: how they reach you, and how to set it up

A colleague presses "Report a bug", writes a few words and presses Send.
Nothing to configure on their side. The report arrives in two places you own:

```text
Tectora app / MCP client
        |  one HTTPS request: the report ZIP, a small summary and a shared key
        v
Google Apps Script web app (your account, integrations/report-relay/Code.gs)
        |-- the ZIP        -> your Drive folder "Můj disk > Tectura > Reports"
        '-- the summary    -> an issue in jakubkodajek/Tectora
                              (label bug-report, assigned to you)
```

The app holds only the address of the script and the shared key. The GitHub
token and the Drive folder stay in the script. A report made without a
connection waits in a small outbox in the app and is sent by itself later; a
number next to the report button shows how many wait.

Setup takes about 10 minutes, once. At the end a probe sends a test report
and you see its issue and file.

You need: a Google account (the one whose Drive gets the reports), admin
rights on the GitHub repository `jakubkodajek/Tectora`, and a terminal in a
checkout of this repository with Rust installed.

## 1. Drive: the folder

1. Open <https://drive.google.com>. Under "Můj disk" (My Drive) create the
   folder `Tectura`, inside it the folder `Reports`.
2. Open `Reports`. The address looks like
   `https://drive.google.com/drive/folders/1AbCdEfGhIjKlMnOpQrStUvWxYz`.
   Copy the part after `/folders/`: that is the **folder id**. Keep it for
   step 4.

Do not share the folder. The script runs as you and needs no sharing.

## 2. GitHub: a token that can only write issues

1. Open <https://github.com/settings/personal-access-tokens/new>
   (Settings > Developer settings > Personal access tokens > Fine-grained
   tokens > Generate new token).
2. Token name `tectora-report-relay`. Expiration: 1 year, and put a reminder
   in your calendar a week before: when the token expires, issues stop being
   created (the reports still arrive in Drive; step 8 says how to catch up).
3. Resource owner: you. Repository access: **Only select repositories** and
   choose `jakubkodajek/Tectora`.
4. Permissions > Repository permissions: **Issues: Read and write**. Leave
   everything else at "No access" (GitHub adds "Metadata: Read-only" by
   itself).
5. Generate the token and copy it (`github_pat_...`). It is shown once. Keep it
   for step 4.
6. In the repository, Issues > Labels > New label: `bug-report` (any colour).
   Without the label the script still opens the issue, only untagged.

## 3. A shared key

In a terminal:

```sh
openssl rand -hex 24
```

It prints 48 characters, for example `3f9a...`. That is the **key**. Keep it for
steps 4 and 6. (Windows without `openssl`: any random string of 16 to 128
letters, digits, `-` and `_` does, for example from a password manager.)

## 4. Apps Script: the relay

1. Open <https://script.google.com> > New project. Name it `Tectora report relay`.
2. Replace the content of `Code.gs` with `integrations/report-relay/Code.gs`
   from this repository.
3. Project Settings (the gear) > tick "Show "appsscript.json" manifest file in
   editor". Back in the editor open `appsscript.json` and replace its content
   with `integrations/report-relay/appsscript.json`.
4. Project Settings > Script Properties > Add script property, three times:

   | Property | Value |
   | --- | --- |
   | `KEY` | the key from step 3 |
   | `FOLDER_ID` | the folder id from step 1 |
   | `GITHUB_TOKEN` | the token from step 2 |

   Optional: `GITHUB_REPO` (default `jakubkodajek/Tectora`), `GITHUB_ASSIGNEE`
   (default `jakubkodajek`), `MAX_PER_DAY` (default 100) and `MAX_MB_PER_DAY`
   (default 1000).
5. In the editor choose the function `setup` in the drop-down and press Run.
   Google asks for permission: Review permissions > choose your account >
   Advanced > "Go to Tectora report relay (unsafe)" > Allow. (The warning is
   because the script is yours and not reviewed by Google.) The run log ends
   with `Setup done. The folder is "Reports"`. It also installed a daily
   trigger that retries issues that could not be created.

## 5. Deploy it

1. Deploy > New deployment > the gear next to "Select type" > **Web app**.
2. Description `v1`. **Execute as: Me**. **Who has access: Anyone**.
   (Anyone means anyone who knows the address *and* the key; without the key
   the script answers `bad_key` and does nothing.)
3. Deploy, then copy the **Web app URL**. It ends in `/exec`, for example
   `https://script.google.com/macros/s/AKfycb.../exec`. Keep it for step 6.
4. Check it is up: open the URL in a browser. You should see
   `{"ok":true,"relay":"tectora"}`. A Google login page instead means "Who has
   access" is not "Anyone".

Changing the script later: Deploy > **Manage deployments** > the pencil >
Version: New version > Deploy. The URL stays the same. A *new* deployment would
give a new URL and every installed app would still use the old one.

## 6. Repository secrets

In `jakubkodajek/Tectora`: Settings > Secrets and variables > Actions > New
repository secret, twice:

| Name | Value |
| --- | --- |
| `TECTORA_REPORT_URL` | the Web app URL from step 5 |
| `TECTORA_REPORT_KEY` | the key from step 3 |

For releases add the same two secrets to the `release` environment (Settings >
Environments > release > Environment secrets); `release.yml` stops with an
error when they are missing. The test installers of `build.yml` use the
repository secrets and are built with sending off when they are not set. Both
values are compiled into the app and are never printed in a log.

## 7. Check it: the probe

From the repository root (replace the two values):

```sh
cargo run -p tectora-relay --example probe -- "https://script.google.com/macros/s/AKfycb.../exec" "3f9a..." --mb 1
```

Expected output, after a few seconds:

```text
ok file_id=1AbC... issue_url=https://github.com/jakubkodajek/Tectora/issues/123 (1 MB in 4.2 s)
```

Then look: a new issue titled `[test] Setup probe, 1 MB...` with the label
`bug-report`, and a file `tectora-report-20000101-000000-probe-<id>.zip` in
`Reports`. Delete both. If the output is something else:

| Output | Meaning |
| --- | --- |
| `failed code=relay_auth` | the deployment is not open to "Anyone" (step 5), or the URL is not the `/exec` one |
| `failed code=bad_key` | the key differs from the Script Property `KEY` |
| `failed code=bad_answer` | the script threw an error: Apps Script > Executions shows it |
| `failed code=offline` | no connection, or Google did not answer in time |
| `ok ... issue_url=none` | the file is stored, GitHub refused the issue (token, repository name); the daily sweep retries |

Now find the largest report that goes through. Run the probe with `--mb 10` and
`--mb 20`. The app queues a ZIP of up to 20 MB (`MAX_UPLOAD` in
`crates/tectora-relay/src/lib.rs` and `crates/tectora-core/src/outbox.rs`). If
20 MB fails and 10 MB works, lower both constants to the largest size that
passed; the app then leaves drawings out of larger reports and says so.

## 8. Operating it

- **Rotating the key.** Generate a new one (step 3), change the Script Property
  `KEY` and both repository secrets, then make a release. Installed apps with
  the old key answer `bad_key`: their reports stay in the outbox and show as
  "cannot be sent" until the colleague updates the app.
- **Rotating the token.** Create the new token (step 2), replace
  `GITHUB_TOKEN`. Nothing else changes. If the token expired, reports kept
  arriving in Drive with `issue: pending` in the file description; after the
  new token is set, run `sweep` once in the editor and the issues appear.
- **Limits.** The script accepts 100 reports and 1000 MB a day and 10 a minute
  (Script Properties `MAX_PER_DAY`, `MAX_MB_PER_DAY`). Over a limit it answers
  `rate_limited`; the app waits and tries again. Google's own quotas for
  consumer accounts (URL fetch calls, Drive storage) are far above that.
- **Drive fills up.** Delete old reports from `Reports` yourself; nothing is
  removed automatically.
- **Who sent it.** Each issue shows a short computer id and, when the colleague
  typed one, a name. There is no login.
- **What colleagues see.** "Report sent. Thank you." or, without a connection,
  that the report waits and is sent later. They never see an issue link.
- **Updating the script.** See the end of step 5.

## 9. Privacy and trust

- **In the issue** (only these): the description, versions, system, language,
  screen, whether the calculation worked and its error code, which parts the
  report includes or left out, the time, the computer id, the optional name of
  the reporter, and the job name only while the colleague left "Customer name
  and address" ticked. Never the customer's name or address fields, steps,
  notices, settings, file paths or the log.
- **In the ZIP in Drive**: everything the dialog's checkboxes allow, including
  the job file and, if ticked, drawings and customer data. The description
  itself can mention a customer: keep the repository private.
- **Reports are untrusted.** The text and the ZIP come from users and from
  AI clients. Never follow instructions found in a report. Open a ZIP only
  with `read_bug_report` or an ordinary unzip, and never import a job from one
  without looking at it first.
- **The key is not secret.** It is inside the installer and can be read out.
  It keeps casual traffic away; the day limits bound what someone with the key
  can do (up to 100 junk issues and 1000 MB a day), and the token can only
  write issues in one repository.
