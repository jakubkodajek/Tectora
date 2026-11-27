# AI integration (MCP)

Tectora ships a [Model Context Protocol](https://modelcontextprotocol.io/specification/2025-06-18)
server so AI clients can create and edit roof designs while the user watches in
the desktop app.

## Starting the server

- Installed app: `<path to Tectora executable> mcp` (the same binary as the GUI;
  with the `mcp` argument it serves MCP on stdin/stdout and opens no window).
- Development / CLI: `tectora mcp [--dir <library folder>]`.

Both use the same project library as the app: `$TECTORA_PROJECTS_DIR` or
`Documents/Tectora`.

## Connecting an AI app

The app's "Connect AI" dialog lists the AI apps installed for this user and
connects each with one click (`tectora_core::assistants`, the approach of
Jakub's Buzzdome app). Connecting adds one entry, `tectora`, with this
executable and the `mcp` argument, to the app's own settings file and keeps
everything else in it (TOML comments too). Nothing else changes; the AI app
must be restarted (or its MCP list refreshed) to pick it up.

| AI app | Settings file | Entry |
| --- | --- | --- |
| Claude Desktop | macOS `~/Library/Application Support/Claude/claude_desktop_config.json`; Windows `%APPDATA%\Claude\`, plus the packaged (MSIX) app's own copies in `%LOCALAPPDATA%\Packages\Claude_*\LocalCache\Roaming\Claude\` and `%LOCALAPPDATA%\Claude-Data\claude_*\` | `mcpServers.tectora` |
| Codex (ChatGPT desktop app, Codex CLI and IDE extension) | `~/.codex/config.toml` or `$CODEX_HOME/config.toml` | `[mcp_servers.tectora]` |
| Antigravity | `~/.gemini/config/mcp_config.json` | `mcpServers.tectora` |

An app counts as installed when the folder it makes on first start exists.
A row says "Needs connecting again" when an entry starts another program
(Tectora moved, or an entry pasted by hand points at an old copy). A settings
file that is not valid JSON/TOML is left untouched and the error is shown.
For any other app the dialog gives the `claude mcp add` command for Claude
Code and the JSON entry to paste. ChatGPT on the web cannot start local
programs; Codex in the ChatGPT desktop app can.

## Tools

| Tool | Purpose |
| --- | --- |
| `tectora_guide` | Model conventions (DACAD terms, units) and the drawing-to-roof workflow |
| `list_projects`, `get_project`, `calc` | Search the library (text, status, AI change, waiting review, dates, area, sort; each job's `review_pending` counts the AI steps that wait for review), read designs, edge tables, dormers, quantities by DACAD line code, per-part warnings, calculation errors and the parts each roof is joined with, stored result |
| | A job too complex to calculate (buildings stacked on each other, work bound of `tectora_core::work`): `calc`, `calc_breakdown`, `export_list` and `export_plan` refuse it with "too complex to calculate"; `get_project` still returns its walls and objects with `too_complex: true` and a `note` and leaves out `quantities`, `placements` and `stored_result`, so the AI can remove the copies (`delete_items`); `list_projects` marks it `too_complex` |
| | Unknown top-level keys of a job (written by another tool) are kept as they are, up to 64 KB and 32 levels of nesting; a job nested deeper is refused everywhere with `too_many: … unknown keys nested too deep` (read, calculation, import), the same as in the app, and a key named `__proto__` is dropped when a job is read. When another program makes a job too deep while it is open in the app, the editor's next save writes its own copy over it, as it repairs a damaged file; the editor keeps a notice up meanwhile ("Another program changed this job"), the status bar does not say saved, and the store's answer (`repaired` on a checked save and on a revert, `replaced_unreadable_file` in an MCP revert) says when that program's file was replaced (the app's notice names the job and stays over the library after Back). A job a newer Tectora writes while it is open is never written over: the editor keeps a notice up ("A newer Tectora saved this job") and its status bar says so instead of saved; `tectora info` refuses such a file as `calc` does (exit 1). A revert to an earlier version outside the limits answers `revert_out_of_limits`, or `revert_out_of_limits_damaged` when the job file itself cannot be read |
| `calc_breakdown` | How every face area and line length was calculated, step by step (the app's Calculation tab) |
| `line_details` | Every roof line's kind, code, length, slope, end heights and the faces it bounds (what the editor shows on hover) |
| `create_standard_roof`, `create_project`, `create_from_example` | Create designs |
| `add_part`, `add_shape_part`, `transform_part`, `delete_items`, `clear_roof` | Parts: add (standard roofs, regular polygons), move / rotate / mirror / copy, delete anything, clear the whole canvas |
| `update_edges`, `set_outline`, `edit_walls` | Walls and their roof definitions (incl. clad gable and the direction of a direction change); corners, lengths, direction changes, parallel shift |
| `set_part_height` | A building's height (`height`, top of the walls; the roof and the objects on it move with it, each eave keeps its offset) or every wall at one eave height (`eave_height`), in one save |
| `add_dormer`, `update_dormer` | Dormers by kind (gable, pointed, hip, half-hip, shed, trapezoid, round), by eave height or, as the dormer window, by front wall height, depth or ridge length; front and side overhangs apart, a trapezoid by its back width |
| `set_openings`, `set_manual`, `face_to_part`, `set_roof_options` | Windows, chimneys, openings (a window larger than its face is refused on every write, not only here; a hand-drawn face pitch is 0 to 89.9, its corners go in order around it (an outline that crosses, touches or folds back over itself is refused), a typed line length 0 or more); hand-drawn lines and faces, a face turned into a building; flat roof, material names |
| `update_opening` | Position, width, depth, rotation and top of one chimney or roof opening (any shape, stretched in its box along the first side), or position and size of one roof window (the object bar) |
| `transform_items` | Move, rotate, copy or align to the nearest wall single dormers, chimneys, windows, openings, hand-drawn lines and faces (the plan's drag and right-click menu) |
| `place_item` | Put one dormer, chimney, window or opening at distances on its roof face: from the eave (along the slope) and from the left or right side edge, as the editor's position fields and plan dimensions show them |
| `roof_helper` | Calculation helpers (triangle, two parallels, point in space, merge, hip/valley, build-up, round roof), preview or apply |
| `update_job`, `store_result` | Job data and status; the stored result for the offer |
| `get_history`, `get_review`, `revert_to`, `revert_last_change`, `show_project` | History (with `kind`, `undid_from`, `undone`, `created`, `pending_review` per entry: an entry with `undone` is not in the design), review state (accepted or still pending in the app, effect on the figures), undo (`revert_last_change` with `ts_ms` undoes only that very change, else `revert_stale`; `revert_to` with a revert's `ts_ms` takes that revert back), focus in the app |
| `restore_offer_result` | Puts an offer's own stored figures back in place of Tectora's recalculation of them, and nothing else (the editor's "Put the offer's figures back") |
| `duplicate_project`, `delete_project`, `list_examples` | Library; `duplicate_project` with `job` starts a new job from a past roof; `delete_project` moves the job to the library's trash (`.trash`, 30 days) |
| `list_deleted`, `restore_project` | Deleted jobs and removed files in the trash, and bringing them back |
| `list_files`, `attach_file`, `remove_file` | Files attached to a job (drawings): regular drawing, photo or document files up to 256 MB; a removed file goes to the trash |
| `export_job`, `import_job`, `get_schema` | `.tectorajob` bundles, file schema; `export_job` of a job whose stored result is Tectora's recalculation of an offer's answers with a `note` naming the recalculated figures the bundle carries, the offer's own that stay in this library, and `restore_offer_result` as the way to send them |
| `export_list`, `export_plan` | The roof for the offer: the text list the Excel price sheet reads (the Export dialog's copy button) and the roof sheet with lengths, areas and pitches as PDF or SVG in the library's `exports` folder (PNG only from the app's Export dialog: the web view rasterises the sheet and the core has no rasteriser; an MCP PNG would need one, e.g. `resvg`, a new dependency). Both run in the calculation worker with its 60 s deadline, and `export_list` does not draw the sheet |
| `get_settings`, `update_settings` | App settings shared with the Settings screen: jobs folder, drawings inbox folder, language, theme, defaults for new roofs, follow mode |
| `list_drawings`, `drawing_info`, `view_drawing`, `drawing_dimensions`, `trace_outline`, `drawing_lines`, `measure_drawing` | Technical drawings (PDF in the drawings inbox or attached to a job): page size and kind, rendered pages and crops with a sheet-mm grid, every dimension chain with its drawn length, an outline snapped to the drawn corners and returned in metres, the exact vector lines and the text-layer labels (filtered, e.g. every height label), measuring between two points against a dimension the AI read, and the calculated roof drawn over the plan (`view_drawing` `overlay`) to check it |

Every write requires a `summary` sentence for the user. The drawing tools only
read; they are implemented in `crates/tectora-drawing` (pure Rust, `hayro`
PDF renderer).

**Arguments.** Every call is checked against the tool's published
`inputSchema` before it runs: an unknown key, a value of the wrong type and
a value outside an `enum` are refused and nothing is saved (number ranges
are checked by each tool, with its own reason). A write to a job past the
draft whose figures then differ from its stored result says so in its
`note` and returns `stored_result.differences`. A stored result an older
Tectora took is recalculated only when the user opens an offer job in the
app (`refresh_stored_result`, before `load_project`), with a change-log
entry by `tectora`; `get_project` and the other MCP
tools only read it. The offer's own figures are put back by that entry
(`Store::restore_offer_result`: the app's button on the entry, MCP
`restore_offer_result`), see "The recalculation and its way back" below. A request line that is
not a JSON object, or has no `method`, is answered with JSON-RPC
`-32600` (invalid request).

**File paths.** The `path` arguments of the drawing tools, `attach_file` and
`import_job` reach only the jobs folder and the drawings inbox, and only
regular files (a folder, device or named pipe is refused before it is
opened). Any other
path is refused, existing or not, and stored as `file_request` in the
settings; the app asks the user ("Allow AI this file?") and an allowed file
is added to `allowed_files` (one file, never its folder). An AI client cannot
set either key: `update_settings` does not take them.

**Settings changes.** `update_settings` applies the language, theme, follow
mode and the defaults for new roofs at once, and records each call that
changed something in `ai_changes` in the settings file (who, the summary,
the changed keys with their values before and after, `state`: `open`,
`accepted`, `reverted`; the last 20 are kept, answered ones dropped first;
when all 20 are still open the two oldest become one change, so an
unanswered change is never lost). A summary is at most 500 characters of
plain text: one of only invisible characters, or with control or
bidirectional characters, is refused, as such characters are in the job's
number, customer and address. The app shows the newest open
change in a card with the summary and before → after, with Revert (puts back
the values it changed; a value changed again since stays) and Accept
(`Settings::answer_ai_change`, Tauri command `answer_ai_change`). The AI tab
lists the recent settings changes with their answer. Only the user answers;
an AI client cannot write `ai_changes`. Folder changes are not in it: they
stay a `folder_request` the user allows or keeps.

## What the user sees

- Every write goes through `Store::save_as`, which appends an entry to
  `.activity/<id>.jsonl` with the actor (the MCP client name; "user" or an empty name is changed to "AI (user)" / "AI", so an AI save is always reviewed), the summary, the
  computed changes and the previous version.
- The server also writes `.activity/live.json` on each call ("Reading …",
  "Calculating …", or the write summary).
- The app watches the folder: it opens the job the agent saves or shows with
  `show_project` (follow mode, can be turned off; reading or calculating
  another job never switches, and while the user is drawing, placing an
  object, typing or has a dialog open the app only offers "Open" in a toast)
  and shows a banner with every pending step's summary, the
  list of changes and how they change the figures (roof area, ridge height,
  line lengths by code, area by material: before → now, with the difference).
  Changed walls, dormers, chimneys, windows, openings and hand-drawn lines and
  faces are highlighted in orange; the previous outline of moved walls and the
  old place of moved or removed items are drawn dashed ("Previous state"
  toggle). Hovering a change in the AI tab shows only that change.
- The changes are computed per item (`tectora_core::diff`): one entry per
  dormer, chimney, window, opening, hand-drawn line or face (`item` index),
  plus flat roof, material names, wall material and the stored result.
- Accept / Revert all act on the whole review at once. The user's Revert
  of one step in the AI tab takes only that step out of the review; the
  others still wait (`PendingReview.steps`, owner decision of round 16). The review state is
  kept in the library (`.activity/<id>.review.json`), so it survives a restart
  and the agent can read it with `get_review`; only the user accepts.
  The AI tab keeps the full history.
- **The review keeps the job as the user last settled it** (ADR 0007,
  audit 2026-11-24 round 20 design Y, superseding the version rule of ADR
  0006, whose log-derived settle point broke after the 50-entry trim and
  after an Undo of Revert all: ARCH-1901, UX-1901, HUNT-1901). From an
  AI's first write since the job was settled until the user's Accept,
  `.activity/<id>.baseline.json` holds the **baseline**: the job before
  that write (for a job an AI created, the job as created, and the creation
  waits), with `since_ts_ms` the AI's entry. **A review is open exactly
  while the baseline exists and the job differs from it** (or the AI's
  creation waits); **Revert all writes the baseline**, logged as the user's
  revert from `since_ts_ms`, so Ctrl+Z takes it back through the log and
  the review is open again. Later AI writes, AI reverts and the user's
  reverts from inside the review leave the baseline as it is; the editor's
  unlogged saves and the user's put-back of the offer's figures are applied
  to it too (a three-way merge), and where the user edited a value the AI
  had set, Revert all refuses with `revert_conflict` naming it; the user's
  own other logged change and the engine's recalculation settle the job
  (no baseline); the user's revert from before `since_ts_ms` (Ctrl+Z past
  work accepted earlier) makes the job it writes the baseline, and its
  Redo brings the old one back; Accept drops it, or, when an AI entry came
  after the accepted time, starts it again before that entry. The steps
  `get_review` and the banner list are for display: the AI's changes from
  `since_ts_ms` on that stand, and the AI's reverts that reach back before
  it (they took settled work out or put it back). `since_ts_ms` may name an
  entry the log no longer holds; `before` is the baseline; the figures are
  the baseline against the job now. The banner's Revert all names the
  review it showed (`revert_changes` with `review: true`,
  `Store::revert_review`): when that review is over by now (accepted,
  reverted, or another one) the answer is `revert_stale` and nothing is
  written (ERR-1905). Another program's unlogged save (an older Tectora on
  the same folder, a sync tool, a hand edit; R10, round 21 ERR-2001, owner
  decision 1) is the colleague's edit: the baseline records a digest of the
  job as Tectora last wrote it and that job itself when the write had no
  log entry (`last_saved`); on every read and before every write a file
  with another digest is folded into the baseline like an editor save,
  from the job Tectora last wrote (or the `before` of an entry another
  program logged), so Revert all keeps the edit and an edit of a value the
  AI set is a conflict. Only when that job is not known (a baseline file of
  round 20) the baseline is marked `outside` and Revert all answers
  `revert_outside` until Accept (the Revert of single steps still works).
  An older Tectora's logged entries are caught up on read, and so is its
  Accept, which writes only the review mark: the file records the mark it
  was written with (`accepted_ms`) and a newer mark is R9 (ARCH-2001). A
  remembered conflict ends when the user puts the AI's value back or types
  the settled one (ERR-2002); the take-back of the take-back of a revert
  past accepted work swaps the baselines again (R7, ARCH-2003); a baseline
  file another job left at the id (an older Tectora's delete) counts as
  missing (ARCH-2006). `review_status` also carries `changes`, the
  baseline against the job now, which the banner's change line and the
  plan's marks show (owner decision 4 of round 21: the net difference
  against the approved state; each step's own change is in the AI tab),
  `before_rev` (a digest of the baseline, for the editor's drawing of the
  roof before) and `steps_stand` (false when the listed steps are history).
  The library card's `review_pending` is the number of waiting steps: the
  library's "waits for review" mark and filter and MCP `list_projects`
  (`review_pending: true`) read it (owner decision 3 of round 21). A job of an older
  Tectora (no baseline file) gets its review from the log once, by the
  round-19 rule, and the file is written at its next write. The file is
  read only as a regular file (never through a link) of bounded size; a
  damaged one counts as missing. It moves with the job to the trash and
  back; Duplicate and a new job from this roof copy a waiting review (the
  copy's log gets the AI's entries from `since_ts_ms`, the baseline gets the
  copy's name and job data: the same banner and Revert all; owner decision
  2 of round 20); a bundle does not carry it (a received job arrives
  reviewed). The change log is trimmed to its newest 50 entries again,
  nothing kept in place. What every revert covers is still read in one
  forward walk of the log (ARCH-1801). Visible consequences: the AI or the
  user putting the job back exactly as settled leaves no banner, and a
  Revert all then has nothing to write.
- **A received job names the sender** (UX-1801). Importing a bundle renames
  the sender's own entries to `user (received)` (shown as "Sender") like
  the engine's to `tectora (received)`: they keep their kinds (revert,
  put-back) but are never this user's decision.
- **A deleted job stays deleted** (ARCH-1702, design E). A write that
  carries a base (every MCP change, `save_as_checked`) is a change of an
  existing job: when the file is gone it answers `not_found` and writes
  nothing, so an AI save that was under way while the user deleted the job
  does not bring it back under its id; MCP tells the AI so. Creation with a
  given id and the editor's own save are unchanged.
- **Undo and Redo of a Revert are logged** (owner decision 2 of round 17,
  UX-1601). Revert all and a one-step Revert answer the revert entry they
  logged (`RevertedChanges.entry_ts_ms`); the editor's Ctrl+Z over that
  step calls `take_back_revert` (`Store::take_back_revert`), a revert of
  the revert, so the AI's steps stand and wait for review again; Redo
  takes the take-back back. Ctrl+Z over an AI's outside save is a revert
  from its oldest new entry (`Store::revert_to_counted_seen`). No Undo
  moves AI work in or out of the job without a log entry. Every such press
  names the newest log entry whose effect the editor has applied
  (`seen_ts`, set only when the editor applies a job read or a core
  answer); the core refuses with `revert_stale`, nothing written, when the
  log has a newer entry (an AI saved during the press), so a key press
  never reverts an AI change the user has not seen. The editor then reads
  the job again (the AI's save comes in as its own step) and says so under
  the Undo / Redo title. Because Undo and Redo are last in, first out, a
  revert from an older entry covers the steps undone after it, so any
  number of presses in a row work (round 18 design B, owner decision 2).
  Presses made while one is with the core are queued and run in order; a
  refusal drops the rest. MCP has the same as
  `revert_to` with the revert's `ts_ms`. The put-back of the offer's
  figures stays an ordinary undo step (UI-802).
- **History marks.** `ActivityItem.undone`: a change a standing revert
  undid; a revert only when it was taken back (the revert that undid it
  starts later than its own range), not when a later revert from further
  back swallowed it (its changes are still out). `created`: the entry that
  created the job, with no version before it; the AI tab draws no Revert on
  it, and Revert all goes back to the job as it was right after the
  creation. When only an AI's creation of the job waits, the banner offers
  Accept or Delete job (to the trash, restorable for 30 days; owner
  decision 3 of round 17). `took_out` / `put_back` (with `took_out_count`
  / `put_back_count`): for a revert, the times of the changes it took out
  and put back, oldest first, at most 20 (also in MCP `get_history`). The
  history decides the user's revert's sentence by them, never by the
  actors along a chain of reverts (UX-1701, round 18 design C): one AI
  change out reads "AI change reverted: “…”", several "AI changes
  reverted"; one put back "AI change put back: “…”", several "Revert of AI
  changes taken back"; both "AI changes: n reverted, m put back".
  `kind` is always sent (`null` for a change, ARCH-1601); the app reads a
  summary's words only for an item without the field.
- A revert keeps the edits made after the reverted step (the app's own
  saves are not logged): the store merges the inverse of the step onto the
  current file, using the version the step saved (`after`) as the base. When
  a later edit changed a value the step also set, the revert is refused with
  `revert_conflict` and the file stays as it is. Revert all undoes the steps
  one by one, latest first, the same way, so an edit the user made between
  two AI steps is kept too (or the whole revert is refused).
- A revert's change-log entry starts with the fixed English sentence the app
  translates (`Reverted: <summary>`, `Reverted <n> changes since: <summary>`);
  a revert of a revert says what it did and quotes the oldest change it
  touched, never the other revert (`Put back: <summary>`, `Put back <n>
  changes since: <summary>`, `Reverted <n> and put back <m> changes since:
  <summary>`, store.rs `revert_summary`, ERR-1703), so Undo and Redo never
  stack "Reverted: Reverted: …"; older entries keep their text;
  an AI's reason follows as ` (reason: <reason>)`. `<n>` counts the entries
  the revert undid: the user's own put-back of the offer's figures after the
  first undone entry stays and is not counted.
- A Revert button undoes the entry it is drawn on: the app sends that
  entry's time (`revert_last_change` with `expectTs`, MCP `ts_ms`) and the
  store refuses with `revert_stale` when another change is the latest by
  now (an AI saved between the draw and the press). The store takes the
  job's lock once for the whole revert (read the log, check the entry,
  merge, write), so an AI save waiting for the lock lands before the
  revert (then it is stale) or after it (then it waits for review), never
  in between (audit 2026-11-23 ERR-1002). Revert, Revert all, the
  put-back of the offer's figures and Accept wait for that lock at most a
  few seconds (`REFRESH_LOCK_WAIT`), then answer `busy` with nothing
  written; Revert all also returns how many entries it undid
  (`revert_changes` gives `{ project, undone }`).
- An AI's reason always ends its revert or put-back entry: the core cuts
  the quoted summary (and a very long reason, leaving the sentence 120
  characters) so `<sentence> (reason: <reason>)` fits the 500-character
  summary cap and ends with the entry's `reason` field.
- **The recalculation and its way back.** Whether a job's stored result is
  the engine's recalculation is one fact in the core,
  `Store::recalculation(id)` (the app's `result_recalculation`): the file's
  stored result has an origin record (`.activity/<id>.result.json`, local,
  never in a bundle) that names the engine's change-log entry and keeps the
  offer's result and the recalculated one, and the recalculated one is the
  file's. The record, not the log, holds the offer's figures, so they can
  be put back after the entry was trimmed from the log (the log keeps 50
  entries); a record of a round-9 build without them reads the entry and
  gets them copied in on the next logged save. The result card's "recalculated" line,
  the history's and the result card's "Put the offer's figures back"
  buttons, `get_project`'s `result_recalculation` and `get_history`'s
  `stored_result_recalculation` come from it alone, wherever the entry is in
  the log by now and whatever the job's status. So a result the user or an
  AI stored since shows neither; an editor Undo that brings the engine's
  result back shows both again; and an entry by `tectora` that came with a
  bundle (renamed `tectora (received)` on import) shows neither.
  `Store::restore_offer_result(id, entry_ts_ms, actor)` replaces only
  `result_snapshot` under the job's lock, refuses with `revert_stale` unless
  the stored result still is that recalculation, logs
  `Offer result restored (Tectora <to> -> <from>)…` with `restores:
  <entry_ts_ms>`, and pins the origin of the result it put back, so no later
  engine recalculates it. The user's restore entry neither ends nor joins an
  AI review, and Revert / Revert all of other entries leave it alone.
- What an entry is comes from its fields, never from its summary, which an
  AI writes freely (audit 2026-11-23 ARCH-1401): `ActivityEntry::kind` is
  `restore` for an entry with `restores`, `revert` for one with
  `undid_from` (or the user's own revert logged by Tectora 0.8.0 and older,
  told by its sentence), `refresh` for the engine's recalculation (also one
  received), and none for a change. The history items, the library card's
  `last_activity` / `changed_by_ai` and MCP `get_history` carry it as
  `kind`. "Revert the latest change" steps over reverts by `undid_from`
  only; an AI save summarised `Reverted: …` is undone as a change. The app
  translates Tectora's sentences only by `kind` and shows an AI's own
  summary in quotation marks.
- Revert all reports two numbers (`RevertedChanges`, audit 2026-11-23
  HUNT-1401, TEST-1402): `undone`, the undone entries that are changes
  (not reverts themselves, at least 1), which the history line
  `Reverted n changes since: …` names; and `undone_ai`, the undone entries
  by an AI client (an actor outside `RESERVED_ACTORS`), which the app
  compares with the AI steps its banner showed for the "reverted too"
  notice. A revert that ends where the job already is writes nothing and
  reports 0 and 0.
  A bundle carries no offer figures (Jakub's decision 2026-11-23): the
  app's export of a recalculated offer warns the user, and MCP `export_job`
  adds a `note` with both sets of figures and the way back
  (`restore_offer_result`, then export again).
- A stored result with an impossible date is left out when a job is read
  (`Store::load_warnings`, code `result_dropped`): the app says so on open,
  MCP `get_project` lists it as `load_warnings`, and the next MCP write,
  which saves the job without it, repeats it in its `note` and
  `load_warnings`. The dropped figures are not kept in the change log
  (Jakub's decision 2026-11-23: the date is impossible, the warnings tell).
- "Changed by AI" is one rule, `store::changed_by_ai`: the newest entry that
  is neither the engine's nor a restore of the offer's figures is an AI
  client's. `ProjectSummary.changed_by_ai` carries it for the library's
  filter, count and badge; `list_projects` `by_ai` reads the same field.
- Opening a job checks without the job's lock whether a recalculation is
  due (status, stored result, origin); only a due one takes the lock, waits
  for it three seconds at most and then opens the job as it is.
- Saves never overwrite each other. The app saves with
  `save_project_checked` (`Store::save_checked`): only if the file still
  holds the version the editor last read or wrote, otherwise it gets the
  current file back, merges its edits onto it (the outside edit wins where
  both changed a value; the local one stays in undo) and saves again. MCP
  writes check the version the tool loaded in the same way
  (`Store::save_as_checked`) and merge onto a user edit saved meanwhile, or
  refuse when both changed the same value. The check and the write run under
  the job's lock file `.activity/<id>.lock`, shared by all processes.
- Tectora manages the library's own folders (`.activity`, `.files`,
  `.files/<id>`, `.trash`, `exports`) and lock files itself. A link or a
  plain file in place of one of them, or a folder with files where a lock
  file belongs, is refused, never written through, read or moved: new jobs,
  saves, deletes, file removals, copies, `list_files` and exports then
  answer `foreign_entry` with the path in the detail. What a link points to
  is kept; the way back is to move that folder into the library in the
  link's place (deleting the link would hide every job's history or files).
  The library screen names such an entry before anything is edited. An
  empty folder, a pipe or a link at a lock file's path is replaced by a plain
  lock file. A received job's history keeps the newest 50 changes, as every
  change log does; `get_history` hands out at most those.

A skill for Claude that describes the drawing workflow is in
`integrations/skills/tectora-roof/SKILL.md`.
