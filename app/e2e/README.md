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

The run uses its own ports (Vite 1440, bridge 7890) and folders under
`target/e2e/` (library, settings, scratch files), so it never touches a
running `npm run preview` or real settings. Each test starts with an empty
library and settings file. Tests run one at a time (one shared library).

## What the harness does

- `serve.mjs` empties `target/e2e`, builds the `tectora` CLI and starts the
  preview (`scripts/preview.mjs`) with `TECTORA_CONFIG_DIR` pointing there.
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
| `drawing.e2e.ts` | footprint by clicks, typed lengths, arrows, keys, presets, cancel, Insert, standard roof tool |
| `objects.e2e.ts` | dormers (all kinds), chimneys, windows, openings, non-convex notice, roof lines, faces, flat roof, transform, delete part, 3D view buttons |
| `job.e2e.ts` | job fields, status and stored result, attachments, export / import, material names, deleted or broken job files |
| `settings.e2e.ts` | language, theme, roof defaults, follow AI, folders, settings changed over MCP |
| `i18n.e2e.ts` | cs / sk / en / de on every screen: no raw keys, no DACAD names, nothing outside the window at 1360×860 and 600×420; dark theme |
| `fields.e2e.ts` | number and text fields: Enter, Tab, Escape, a click on the plan / another wall / a button stores on the right object, decimal comma, refused values, ▲▼ and arrow keys, presets; manual face labels and "make it a building" |
| `joined.e2e.ts` | a second building: roof at once, "joined to" in the results, dashed lines under a higher roof, a covered building warned, recalculation of both roofs |
| `lines.e2e.ts` | line lengths by code (T, F, G, K, O, P, c) in the Results tab equal the core's totals and per-code list of the saved file, also after an edit |
| `mcp.e2e.ts` | live AI review (banner, highlight, Accept, Revert, Revert all, follow mode, library marks, races with autosave) and MCP parity: the same edit in the editor and over MCP gives the same file and result |

Most assertions check the saved file (`waitForProject`) and that the area in
the Results tab equals the core's calculation of that file.

## Not covered here

Native parts need `npx tauri dev` or an installer on macOS and Windows: file
dialogs themselves, opening attachments and folders in the OS, double-click
file association, window chrome, the updater, and WebKit (macOS) / WebView2
(Windows) rendering differences.
