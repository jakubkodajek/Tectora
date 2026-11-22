# End-to-end tests

The whole UI in Chromium against the real Rust core, plus the MCP server an
AI client talks to. Nothing is mocked except what only Tauri has (native
windows): the browser preview's `ui_bridge` answers the same commands as
`app/src-tauri/src/lib.rs` over a real library folder.

```bash
cd app
npm run e2e                       # all tests (builds ui_bridge + tectora CLI first)
npx playwright test library       # one file
npx playwright test -g "MCP parity"
npm run e2e:report                # HTML report after a CI-style run
```

The run uses its own ports and folder (library, settings, scratch files),
so it never touches a running `npm run preview` or real settings. Each test
starts with an empty library and settings file. Tests run one at a time
within a run (one shared library).

### Several runs on one machine

Each run claims a free slot when Playwright loads its config (`env.ts`):
`target/e2e-slots/<n>.pid` holds the runner's pid and is removed when the
run ends (a slot whose process is gone is taken over). The slot gives the
run's folder and ports, so two runs (two agents, two terminals) never wipe
each other's library:

| slot | folder | Vite | bridge | Playwright output / report |
| --- | --- | --- | --- | --- |
| 0 (the first run) | `target/e2e` | 1440 | 7890 | `target/e2e-results`, `target/e2e-report` |
| n | `target/e2e-<n>` | 1440 + n | 7890 + n | `target/e2e-results-<n>`, `target/e2e-report-<n>` |

A single run (`make e2e`, CI) is slot 0, as before. Override any value with
`TECTORA_E2E_DIR`, `TECTORA_E2E_PORT` (Vite) and `TECTORA_E2E_BRIDGE_PORT`,
or pick a slot with `TECTORA_E2E_SLOT=<n>` (not claimed then). `serve.mjs`
empties the run's folder only when it is a `target/e2e*` folder or one it
made itself (marker file `.tectora-e2e`), so a typo in `TECTORA_E2E_DIR`
cannot wipe other files. Two full runs at once are slower (each builds and
runs Chromium), so agents that share a machine still queue with
`flock /tmp/tectora-e2e.lock npx playwright test …`.

## What the harness does

- `env.ts` claims the run's slot (folder and ports, above) and passes it to
  the web server and the workers through the environment.
- `serve.mjs` empties the run's folder, builds the `tectora` CLI and starts
  the preview (`scripts/preview.mjs`) with `TECTORA_CONFIG_DIR` pointing
  there.
- `fixtures.ts`: the `app` fixture opens the page with Tauri's dialog
  behaviour (`window.confirm` is asynchronous, as with Tauri's dialog plugin),
  answers native file dialogs from a queue (`answerFileDialog`) and fails a
  test on any page or console error. The `mcp` fixture is a stdio MCP client
  (`mcp.ts`) connected as "Claude".
- `editor.ts`: toolbar and bar lookups by translation key, and `clickPlan(x, y)`
  in world metres (calibrated from the status bar's cursor read-out).
- `i18n.ts` reads the app's dictionaries, so tests find text in any locale.

## Files

| file | covers |
| --- | --- |
| `library.e2e.ts` | new job (all start modes and standard types), examples, search, filters, sort, list / grid, detail pane, card menu, duplicate, delete, import, broken files |
| `editor.e2e.ts` | layout, F3, fit, dimensions, rename, undo / redo and shortcuts, dock tabs, status bar, every roof definition control, corner drag and insert, calculation and round-roof dialogs, keyboard |
| `drawing.e2e.ts` | footprint by clicks, typed lengths, arrows, keys, presets, cancel, Insert, finish by double-click, button and Ctrl+I, standard roof tool |
| `objects.e2e.ts` | dormers (all kinds), chimneys, windows, openings, non-convex notice, roof lines, faces (a manual face is drawn neutral, not like a building: edge-cases class 4), flat roof, transform, delete part, 3D view buttons |
| `planflow.e2e.ts` | the plan and its bars: eave lines at the overhang, wall lengths while drawing, two buildings' wall labels apart and numbered "2.3", PageDown / PageUp / Alt+arrows / Shift+Enter step walls, typed wall angle (absolute or from the previous wall), the placing tools' own bar, radio groups by arrow keys, the transform copy limit |
| `placement.e2e.ts` | distances from the eave and the sides (fields, plan dimensions, refusals), Properties dialog by double-click (Cancel / OK), building height vs eave height, one bad building leaves the others their roof |
| `job.e2e.ts` | job fields, status and stored result, attachments, export / import, material names, deleted or broken job files |
| `settings.e2e.ts` | language, theme, roof defaults, follow AI, folders, settings changed over MCP, toolbar labels switch |
| `i18n.e2e.ts` | cs / sk / en / de on every screen: no raw keys, no DACAD names, nothing outside the window at 1360×860 and 600×420; dark theme |
| `fields.e2e.ts` | number and text fields: Enter, Tab, Escape, a click on the plan / another wall / a button stores on the right object, decimal comma, refused values, ▲▼ and arrow keys, presets; manual face labels and "make it a building" |
| `joined.e2e.ts` | a second building: roof at once, "joined to" in the results, dashed lines under a higher roof, a covered building warned, recalculation of both roofs |
| `lines.e2e.ts` | line lengths by code (T, F, G, K, O, P, c) in the Results tab equal the core's totals and per-code list of the saved file, also after an edit |
| `mcp.e2e.ts` | live AI review (banner, highlight, Accept, Revert, Revert all, follow mode, `show_project`, library marks, races with autosave) and MCP parity: the same edit in the editor and over MCP gives the same file and result, and both sides changed the job (walls, heights, breaks, corners, job data, flat roof, part transform, chimney / window, dormer add and edit, standard roof, stored result, delete part, roof lines and faces, build-up helper, round roof, distances, object size, object rotation, face to building, standard roof as a part, clear) |
| `recalc.e2e.ts` | edge-cases class 2: after undo, redo, an AI save, a merge on save, a standard roof, a face made a building, a move, a rotation, a mirrored copy, a window / chimney / dormer, flat roof on and off, deleting a part, Revert in the banner, the round roof and a calculation helper, the shown area equals the core's result of the saved file and the roof lines follow |
| `bridge.e2e.ts` | the preview bridge answers only the preview page (other origins, DNS rebinding) |
| `clarity.e2e.ts` | the wall and dormer bars' More row, one word for a building, Back / undo / delete labels, the Excel text in Export, objects listed in the Results |
| `editor-ux.e2e.ts` | Delete asks before removing a corner, the selected wall stands out, breakdown without float noise, manual face in 3D, a double click on Duplicate / Delete acts once |
| `results.e2e.ts` | the Results area list (parts and hand-drawn faces add up), a part's name apart from its warning, objects the calculation leaves out |
| `shell.e2e.ts` | toolbar and library filters at laptop and half-screen sizes, the library list by keyboard and double click, dialog focus and Escape, toggle and tab state for assistive tech, roof defaults, the empty-filter message |
| `audit-r4-ed.e2e.ts` | the one rule for edits that wait for the core (`src/lib/jobedits.ts`): a value lands on the item it was typed for when an AI save deletes an earlier one, and is refused when the AI deletes that item; actions after an ordinary edit plus an AI save of the notes; quick arrow steps and two fields in the chimney and wall bars; up then down keeps the last value; double clicks on dialog OKs and on a button that opens a dialog; re-offer with a kept result; a job of hand-drawn faces only; the empty bar at 600×420; the calculating mark |
| `audit-r8-ed.e2e.ts` | round-8 editor guards: a stored offer result recalculated on open (notice, old figures, the card's dates, Revert), a press on a roof line, + / − zoom and the long press menu on touch, the 3D selection mark |
| `audit-r9-recalc.e2e.ts` | round-9 guards for the recalculated offer result: the put-back acts on its own entry (an AI save before the press, `revert_stale`), a result stored since, Undo across the put-back, the button after an AI save, an engine entry the library did not write, a job left while it is read, the notice at 800x600, line lengths only, the library's changed-by-AI list, the loading state |
| `saveflows.e2e.ts` | revert keeps the user's later edits or refuses on a conflict, an outside save after a failed save, export after a failed save, follow mode waits while the user draws, types or has a dialog open |
| `guards.e2e.ts` | nothing lost or done twice: leaving waits for a window size or building height still with the core, Accept / Revert run once, Revert all asks, a half-drawn footprint asks before another tool or Back, no export or offer from a roof the core refused, the standard roof dialog adds the picked type, ResizeObserver noise shows no toast, Enter creates a new job |
| `textfields.e2e.ts` | edge-cases class 1 for text fields (Job tab, title): Escape restores, typing survives an AI save, empty job name refused, Back right after typing stores |

Most assertions check the saved file (`waitForProject`) and that the area in
the Results tab equals the core's calculation of that file.

## Not covered here

Native parts need `npx tauri dev` or an installer on macOS and Windows: file
dialogs themselves, opening attachments and folders in the OS, double-click
file association, window chrome, the updater, and WebKit (macOS) / WebView2
(Windows) rendering differences.

What of the native path is tested elsewhere: the files an OS open hands to
the app (`file_args`, unit test in `app/src-tauri/src/lib.rs`, run on
Windows CI) and what the core does with them (`crates/tectora-core/tests/open_files.rs`:
any case of the extension, bundles, spaces and diacritics, folders and
missing files refused). Still only by hand, on macOS and on Windows:

- double-click a `.tectora` and a `.tectorajob` file with the app closed and
  with it open (second instance): the job opens, a broken file says why;
- an MCP edit while the job is open in the shipped app: the live banner
  appears (the folder watcher's `projects-changed`, one per save);
- switching the jobs folder in Settings: the library reloads, later outside
  saves still show live;
- Quit with unsaved edits (Cmd+Q on macOS, closing the window on Windows):
  the edits are saved or the app asks (ERR-022);
- the updater: an available update is offered and installs.
