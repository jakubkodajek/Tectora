// Round 12 (audit 2026-11-23 fix design 1; HUNT-1101, ERR-1101, UI-1101,
// HUNT-1103, ERR-1105, TEST-1105): while a writer of the job is out (a drag,
// Revert, Revert all, the offer put-back) the editor takes no edit. Its own
// refusal lives in its DOM (inert, refuseInput, its key handler); anything at
// window level, any toast action and any way out of the editor passes it
// unless it asks the module gate (dragsession.ts `anyWriterOpen`,
// `gatedKeys`). This scan lists every such entry point in app/src: a new
// one fails here until it is gated or written down as passive with a reason.
//
// Round 13 (TEST-1201, HUNT-1201, ERR-1201): listeners are counted per
// file, receiver and event, of every shape (`addEventListener` on any
// receiver, svelte/events `on`, `.onx =`, `<svelte:window>`), and a gated
// one must ask the gate in its own handler before it changes anything; toast
// actions of every shape are counted; every assignment of `openId` and every
// use of `libraryChanged` in App.svelte is listed with its reason, and the
// Tauri event subscriptions too.
import { describe, expect, it } from "vitest";

const sources = import.meta.glob(["../**/*.svelte", "../**/*.ts", "!../**/*.test.ts", "!../dev/**"], { query: "?raw", import: "default", eager: true }) as Record<string, string>;
const files = Object.fromEntries(Object.entries(sources).map(([k, v]) => [k.replace(/^\.\.?\//, "").replace(/^lib\//, ""), v]));

type Entry = ({ gated: true } | { passive: string }) & { count?: number };
const INSIDE = "on an element inside the editor or a dialog: refused there while a writer is out (inert / refuseInput)";

/**
 * Every listener: `file:receiver:event` (receiver `<svelte:window>` etc. for
 * the tags, `*expr` for an event that is not a literal), how many there are
 * (default 1), and `gated` (its handler asks the gate first) or `passive`
 * (writes nothing, or is refused elsewhere), with why.
 */
const LISTENERS: Record<string, Entry> = {
  "App.svelte:<svelte:window>:keydown": { gated: true },
  "App.svelte:window:beforeunload": { passive: "browser preview only: warns about unsaved edits, writes nothing" },
  "App.svelte:window:error": { passive: "shows an unexpected error, writes nothing" },
  "App.svelte:window:unhandledrejection": { passive: "shows an unexpected error, writes nothing" },
  "Editor.svelte:<svelte:window>:keydown": { gated: true },
  "DrawBar.svelte:<svelte:window>:keydown": { gated: true },
  "Modal.svelte:<svelte:window>:keydown": { gated: true },
  "ContextMenu.svelte:<svelte:window>:keydowncapture": { gated: true },
  "ContextMenu.svelte:<svelte:window>:pointerdowncapture": { passive: "closes the menu only" },
  "ContextMenu.svelte:<svelte:window>:blur": { passive: "closes the menu only" },
  "ContextMenu.svelte:<svelte:window>:resize": { passive: "closes the menu only" },
  "ContextMenu.svelte:<svelte:window>:wheel": { passive: "closes the menu only" },
  "commitfocus.ts:win:pointerdown": { gated: true },
  "numeric.svelte.ts:window:pointerdown": { gated: true },
  "itemstep.ts:window:keydown": { gated: true },
  "dblclick.ts:window:pointerdown": { passive: "remembers where the last press was, writes nothing" },
  "dblclick.ts:node:click": { passive: "tells a double click from a single one; the click itself is refused in the editor" },
  "Home.svelte:<svelte:window>:focus": { passive: "the library screen, not mounted with the editor" },
  "ProjectCard.svelte:<svelte:window>:click": { passive: "the library screen, not mounted with the editor; closes a card menu" },
  "RoofDefBar.svelte:el:keydown": { passive: INSIDE },
  "hscroll.ts:node:wheel": { passive: "scrolls a bar sideways, writes nothing" },
  "numeric.svelte.ts:b:mousedown": { passive: INSIDE },
  "numeric.svelte.ts:b:click": { passive: INSIDE },
  "numeric.svelte.ts:input:input": { passive: INSIDE },
  "numeric.svelte.ts:input:keydown": { passive: INSIDE },
  "numeric.svelte.ts:input:focus": { passive: INSIDE },
  "numeric.svelte.ts:input:blur": { passive: "commits the field; the claim of a core action waits while a field holds typed text (Editor `quiet`)" },
  "textfield.ts:el:input": { passive: INSIDE },
  "textfield.ts:el:keydown": { passive: INSIDE },
  "textfield.ts:el:blur": { passive: "commits the field; the claim of a core action waits while a field holds typed text (Editor `quiet`)" },
  "theme.svelte.ts:media:change": { passive: "follows the system's light / dark theme, writes nothing" },
  "Roof3D.svelte:el:dblclick": { passive: INSIDE },
  "Roof3D.svelte:el:pointerdown": { passive: INSIDE },
  "Roof3D.svelte:el:pointerup": { passive: INSIDE },
  "Roof3D.svelte:el:contextmenu": { passive: INSIDE },
  "tabs.ts:node:keydown": { passive: "moves between tabs, writes nothing" },
  "dragsession.ts:node:*type": { passive: "the refusal itself (refuseInput)" },
  "api.ts:progress:message": { passive: "the update download's progress, writes nothing" },
};

/** What calls the gate (the editor's own predicate counts for the editor). */
const GATE = /\b(anyWriterOpen|gatedKeys|inputClosed)\(/;

const stripComments = (s: string) => s.replace(/\/\*[\s\S]*?\*\//g, "").replace(/(^|[^:"'`\\])\/\/[^\n]*/g, "$1");

/** The text from `at` (an opening bracket) to its closing one. */
function balanced(src: string, at: number): string {
  const open = src[at];
  const close = open === "{" ? "}" : open === "(" ? ")" : "]";
  let depth = 0;
  for (let i = at; i < src.length; i++) {
    if (src[i] === open) depth++;
    else if (src[i] === close && --depth === 0) return src.slice(at, i + 1);
  }
  throw new Error("unbalanced");
}

/** The `<svelte:window …/>`-like tag starting at `at`, braces balanced. */
function tagAt(src: string, at: number): string {
  let depth = 0;
  for (let i = at; i < src.length; i++) {
    const c = src[i];
    if (c === "{") depth++;
    else if (c === "}") depth--;
    else if (depth === 0 && c === ">") return src.slice(at, i + 1);
  }
  throw new Error("unclosed tag");
}

interface Found {
  key: string;
  /** The handler: an inline function's text, or the name of one. */
  handler: string;
}

function listeners(): Found[] {
  const found: Found[] = [];
  for (const [file, raw] of Object.entries(files)) {
    const src = stripComments(raw);
    for (const m of src.matchAll(/<svelte:(window|document|body)\b/g)) {
      const tag = tagAt(src, m.index!);
      const recv = `<svelte:${m[1]}>`;
      // A spread hides its listeners from this scan.
      if (/\{\s*\.\.\./.test(tag)) found.push({ key: `${file}:${recv}:...spread`, handler: "" });
      // The shorthand `{onkeydown}`.
      for (const a of tag.matchAll(/\s\{on([a-z]+)\}/g)) found.push({ key: `${file}:${recv}:${a[1]}`, handler: `on${a[1]}` });
      // `onkeydown={…}` and the old `on:keydown={…}`.
      for (const a of tag.matchAll(/\son:?([a-z]+)=\{/g)) {
        const value = balanced(tag, a.index! + a[0].length - 1).slice(1, -1).trim();
        found.push({ key: `${file}:${recv}:${a[1]}`, handler: value });
      }
    }
    // `x.addEventListener(event, handler…)` on any receiver, any quote.
    for (const m of src.matchAll(/([\w$]+)\??\s*\.\s*addEventListener\(/g)) {
      const args = balanced(src, m.index! + m[0].length - 1).slice(1, -1);
      const [ev, handler = ""] = args.split(/,(?![^(]*\))/).map((s) => s.trim());
      const lit = ev.match(/^(["'`])(\w+)\1$/);
      found.push({ key: `${file}:${m[1]}:${lit ? lit[2] : `*${ev}`}`, handler });
    }
    // svelte/events `on(target, event, handler)`.
    for (const m of src.matchAll(/(?<![\w$.])on\(\s*([\w$.]+)\s*,\s*([^,]+),\s*/g)) {
      const lit = m[2].trim().match(/^(["'`])(\w+)\1$/);
      found.push({ key: `${file}:${m[1]}:${lit ? lit[2] : `*${m[2].trim()}`}`, handler: src.slice(m.index! + m[0].length, m.index! + m[0].length + 400) });
    }
    // `x.onkeydown = handler`.
    for (const m of src.matchAll(/([\w$]+)\??\.on([a-z]+)\s*=(?![=>])\s*/g)) found.push({ key: `${file}:${m[1]}:${m[2]}`, handler: src.slice(m.index! + m[0].length, m.index! + m[0].length + 400) });
  }
  return found;
}

/** The definition of `name` in `src` (a function or a const), from its start; null when there is none. */
function definition(src: string, name: string): string | null {
  const m = new RegExp(`(?:function\\s+${name}\\s*\\(|(?:const|let)\\s+${name}\\s*=)`).exec(src);
  if (!m) return null;
  return src.slice(m.index, m.index + 4000);
}

/**
 * Whether `code` (a handler, from its start) asks the gate before it does
 * anything: no assignment and no call other than the event's own
 * preventDefault / stopPropagation lies before the gate's call.
 */
function gateFirst(code: string): boolean {
  const text = stripComments(code);
  const g = text.search(GATE);
  if (g < 0) return false;
  // A wrapper (`const onkey = gatedKeys(…)`): the gate is the handler.
  const head = text.indexOf("{");
  let before = text.slice(0, g);
  if (head >= 0 && head < g) before = text.slice(head + 1, g);
  else if (/^(?:const|let)\s+[\w$]+\s*=$/.test(before.trim())) return true;
  // Declarations of locals are reads.
  before = before.replace(/\b(?:const|let|var)\s+[\w$]+\s*(?::\s*[^=]+)?=/g, "");
  if (/[\w$\])]\s*(?:[+\-*/|&]?=)(?![=>])/.test(before)) return false;
  for (const c of before.matchAll(/([\w$]+)\s*\(/g)) {
    if (!["if", "for", "while", "switch", "return", "typeof", "preventDefault", "stopPropagation", "stopImmediatePropagation"].includes(c[1])) return false;
  }
  return true;
}

/** The handler's code: an inline function as written, else the named function's definition in the file. */
function handlerCode(file: string, handler: string): string | null {
  const h = handler.trim();
  if (/^[\w$]+$/.test(h)) return definition(stripComments(files[file]), h);
  return h;
}

describe("the writer gate covers every entry point outside the editor's refusal (round 12, fix design 1)", () => {
  it("every listener is listed with its count, and the gated ones ask the gate in their handler before anything else", () => {
    const found = listeners();
    expect(found.length).toBeGreaterThan(30);
    const counts = new Map<string, number>();
    for (const f of found) counts.set(f.key, (counts.get(f.key) ?? 0) + 1);
    const unknown = [...counts.keys()].filter((k) => !(k in LISTENERS)).sort();
    expect(unknown, "a new listener: gate it (anyWriterOpen / gatedKeys) or list it as passive with a reason").toEqual([]);
    for (const [key, how] of Object.entries(LISTENERS)) {
      expect(counts.get(key), `listed but not found: ${key}`).toBeTruthy();
      expect(counts.get(key), `${key}: a second listener of the same event (list it, gated or with a reason)`).toBe(how.count ?? 1);
      const file = key.slice(0, key.indexOf(":"));
      if ("gated" in how) {
        for (const f of found.filter((x) => x.key === key)) {
          const code = handlerCode(file, f.handler);
          expect(code, `${key}: its handler ${f.handler.slice(0, 40)} is not found`).toBeTruthy();
          expect(gateFirst(code!), `${key}: the handler does not ask the gate before it acts`).toBe(true);
        }
      } else expect(how.passive.length).toBeGreaterThan(5);
    }
  });

  it("the scan sees the listener shapes it is meant to see", () => {
    // Shapes a regression could take (TEST-1201, HUNT-1206); each is found with its key.
    const shapes: [string, string][] = [
      ['document.body.addEventListener("keydown", k);', "body:keydown"],
      ["window.addEventListener('keydown', k);", "window:keydown"],
      ["window.addEventListener(`keydown`, k);", "window:keydown"],
      ['globalThis.addEventListener("keydown", k);', "globalThis:keydown"],
      ['on(window, "keydown", k);', "window:keydown"],
      ['on(document.body, "keydown", k);', "document.body:keydown"],
      ["window.onkeydown = k;", "window:keydown"],
      ["<svelte:window on:keydown={k} />", "<svelte:window>:keydown"],
      ["<svelte:window {...handlers} />", "<svelte:window>:...spread"],
    ];
    for (const [code, key] of shapes) {
      files["__shape.svelte"] = code;
      try {
        expect(listeners().map((f) => f.key), code).toContain(`__shape.svelte:${key}`);
      } finally {
        delete files["__shape.svelte"];
      }
    }
    expect(gateFirst("(e) => { settingsOpen = true; if (anyWriterOpen()) return; }")).toBe(false);
    expect(gateFirst("(e) => { if (e.key === ',') { e.preventDefault(); if (anyWriterOpen()) return; open = true; } }")).toBe(true);
    expect(gateFirst("function k(e) { hold(1); if (anyWriterOpen()) return; }")).toBe(false);
    expect(gateFirst("const onkey = gatedKeys((e) => { run(); })")).toBe(true);
  });

  it("every Tauri event subscription is listed with its reason", () => {
    const names = [...files["api.ts"].matchAll(/export function (on[A-Z]\w+)\(/g)].map((m) => m[1]);
    expect(names.length).toBeGreaterThanOrEqual(5);
    const SUBS: Record<string, string> = {
      "App.svelte:onProjectsChanged": "reloads the library list, writes nothing",
      "App.svelte:onLiveStatus": "follow mode: leaves only through mayLeave (openId list below)",
      "App.svelte:onSettingsChanged": "the folder switched outside this window: libraryChanged, listed below",
      "Editor.svelte:onProjectsChanged": "an outside save: the reload waits for a drag or core action (defer)",
      "startup.ts:onOpenProject": "a file opened from the system: openJob, which asks mayLeave",
      "startup.ts:onOpenFailed": "shows why a file could not be opened, writes nothing",
    };
    const seen = new Map<string, number>();
    for (const [file, raw] of Object.entries(files)) {
      if (file === "api.ts") continue;
      const src = stripComments(raw);
      for (const n of names) {
        // Calls, not the interface's declarations (`onOpenProject(heard: …)`).
        const k = [...src.matchAll(new RegExp(`\\b${n}\\((?!\\w+:)`, "g"))].length;
        if (k) seen.set(`${file}:${n}`, k);
      }
    }
    expect([...seen.keys()].sort(), "a new subscription to a Tauri event: list it with its reason").toEqual(Object.keys(SUBS).sort());
    for (const [k, n] of seen) expect(n, `${k}: subscribed more than once`).toBe(1);
  });

  it("every toast action is listed, of any shape, and they all run through the gated press of Toasts.svelte", () => {
    const actions: string[] = [];
    for (const [file, raw] of Object.entries(files)) {
      if (file === "notify.svelte.ts") continue;
      const src = stripComments(raw);
      for (const m of src.matchAll(/(?<![\w$.])notify\(/g)) {
        const arg = balanced(src, m.index! + m[0].length - 1).slice(1, -1).trim();
        // An options object built elsewhere would hide its action.
        expect(arg.startsWith("{"), `${file}: notify( with something other than an object literal: ${arg.slice(0, 40)}`).toBe(true);
        // A spread hides an action from the count (TEST-1305).
        expect(arg.includes("..."), `${file}: a spread inside notify( ${arg.slice(0, 40)}`).toBe(false);
        const keys = [...arg.matchAll(/(?<![\w$.])action\s*(?=[:,}]|$)/g)].length;
        const named = [...arg.matchAll(/(?<![\w$.])action:\s*\{\s*label:\s*t\("([\w.]+)"/g)];
        expect(keys, `${file}: a toast action of another shape (shorthand, variable)`).toBe(named.length);
        for (const n of named) actions.push(`${file}:${n[1]}`);
      }
      // A toast's action set outside a notify( call (a Toast object built by hand).
      const outside = [...src.matchAll(/(?<![\w$.])action:\s*\{\s*label:/g)].length;
      const inside = actions.filter((a) => a.startsWith(`${file}:`)).length;
      expect(outside, `${file}: a toast action outside notify(`).toBe(inside);
    }
    expect(actions.sort()).toEqual(["App.svelte:ai.followOpen", "Editor.svelte:snap.refreshedShow", "Home.svelte:home.restore", "JobPanel.svelte:snap.store"]);
    const toasts = stripComments(files["Toasts.svelte"]);
    const run = definition(toasts, "runAction");
    expect(run && gateFirst(run), "Toasts runAction asks the gate before anything else").toBe(true);
    expect(toasts.match(/action\.run/g)).toEqual(["action.run"]);
    expect(toasts).toContain("runAction(toast.id, action.run)");
  });

  it("the editor's commit and undo step refuse a write while a core action is out, and the dialogs and menus opened before it act on nothing", () => {
    const editor = stripComments(files["Editor.svelte"]);
    const body = (src: string, name: string) => {
      const at = src.indexOf(`function ${name}(`);
      expect(at, name).toBeGreaterThan(0);
      return src.slice(at, src.indexOf("\n  }\n", at));
    };
    expect(body(editor, "commit")).toMatch(/dropForeign\(reviewing, "commit"/);
    expect(body(editor, "restore")).toMatch(/if \(dropForeign\(reviewing, "undo step"[^\n]*\)\) return false;/);
    expect(editor).toMatch(/new CoreAction\([\s\S]*?import\.meta\.env\.DEV,?\s*\)/);
    // A refused restore leaves the history whole (ERR-1201).
    const close = body(editor, "closeObjDlg");
    expect(close).toMatch(/if \(!restore\(objDlgAt\)\) return;\s*history = history\.slice/);
    // A menu entry clicked after the call went out runs nothing (ERR-1201).
    const menu = stripComments(files["ContextMenu.svelte"]);
    expect(gateFirst(definition(menu, "run")!), "ContextMenu run() asks the gate first").toBe(true);
  });

  it("the prepare phase stores the focused field once, at the press, and claims only a quiet editor (round 13 design A)", () => {
    const editor = stripComments(files["Editor.svelte"]);
    const at = editor.indexOf("async function prepareReview(");
    expect(at).toBeGreaterThan(0);
    const prep = editor.slice(at, editor.indexOf("\n  }\n", at));
    const loop = prep.indexOf("for (;;)");
    expect(prep.match(/commitFocused\(\)/g), "one commit of the focused field").toHaveLength(1);
    expect(prep.indexOf("commitFocused()"), "before the loop, never in it (UI-1201)").toBeLessThan(loop);
    expect(prep).toMatch(/if \(quiet\(\)\) return runReview\(/);
    const quiet = editor.slice(editor.indexOf("const quiet = () =>"), editor.indexOf("function reviewTransaction("));
    for (const part of ["pendingEdits.size === 0", 'saveState === "saved"', "!drag.active", "!isTyping(document.activeElement)", "questions === 0", "!menu", "openModals() === 0"]) expect(quiet, part).toContain(part);
    // Each confirm whose answer ends in an edit is counted as a question (HUNT-1204).
    for (const name of ["deleteVertexByKey", "deleteDormer", "clearRoof", "deletePart"]) expect(body2(editor, name), name).toMatch(/^[^{]*\{\s*return asking\(async \(\) => \{/);
    // The deadline names the editor's own edits (UX-1201); `busy` stays the core's.
    expect(prep).toContain('code: "busy_edits"');
    expect(prep).not.toContain('code: "busy"');
  });

  it("every way out of the editor is listed and waits for the whole Revert press (leaveGuard)", () => {
    const sites: string[] = [];
    for (const [file, raw] of Object.entries(files)) {
      if (file === "leave.ts") continue;
      const n = [...stripComments(raw).matchAll(/\bmayLeave\(/g)].length;
      if (n) sites.push(`${file}:${n}`);
    }
    // App: follow mode, openJob (Back, follow "Open", a file opened from the system), the window's close;
    // Settings: the folder field; FolderRequest: an AI's folder switch allowed (HUNT-1201).
    expect(sites.sort()).toEqual(["App.svelte:3", "FolderRequest.svelte:1", "SettingsDialog.svelte:1"]);
    const folder = stripComments(files["FolderRequest.svelte"]);
    const answer = folder.slice(folder.indexOf("async function answer("));
    expect(answer.indexOf("await mayLeave()"), "asked before the core switches the folder").toBeGreaterThan(0);
    expect(answer.indexOf("await mayLeave()")).toBeLessThan(answer.indexOf("api.answerFolderRequest("));
    const settings = stripComments(files["SettingsDialog.svelte"]);
    expect(settings).toMatch(/if \(field === "projects_dir" && !\(await mayLeave\(\)\)\) return false;/);
    const editor = stripComments(files["Editor.svelte"]);
    const guard = editor.slice(editor.indexOf("async function leaveGuard("), editor.indexOf("setLeaveGuard(leaveGuard"));
    expect(guard).toMatch(/if \(reviewPress \|\| reviewing\.active\) \{\s*if \(mode === "quiet"\) return false;\s*await reviewPress;\s*await reviewing\.settled\(\);/);
  });

  it("every assignment of openId and every use of libraryChanged in App.svelte is listed with its reason", () => {
    const app = stripComments(files["App.svelte"]);
    const markup = app.indexOf("</script>");
    /** Where `i` is: the top-level function of the script, or the markup attribute. */
    const where = (i: number) => {
      if (i > markup) return `markup:${[...app.slice(0, i).matchAll(/\s(on[\w]+)=\{/g)].pop()?.[1]}`;
      const fns = [...app.slice(0, i).matchAll(/\n {2}(?:async\s+)?(?:function\s+)?([\w$]+)\s*\(/g)];
      const f = fns.pop();
      return f ? { name: f[1], from: f.index! } : "top";
    };
    /** `leaves`: the editor is left only after `mayLeave` in the same function; else why it need not ask. */
    const OPEN_ID: Record<string, "leaves" | string> = {
      onLive: "leaves",
      openJob: "leaves",
      libraryChanged: "the folder switched already (every caller is listed below): the editor closes and tells (err.library_changed)",
      "markup:onopen": "the library screen opens a job: no editor is open",
    };
    const seen: string[] = [];
    for (const m of app.matchAll(/(?<!let\s+)\bopenId\s*=(?![=>])/g)) {
      const w = where(m.index!);
      const key = typeof w === "string" ? w : w.name;
      seen.push(key);
      if (!(key in OPEN_ID)) continue;
      if (OPEN_ID[key] === "leaves") {
        expect(typeof w, key).toBe("object");
        const fn = app.slice((w as { from: number }).from, m.index!);
        expect(fn, `${key}: openId is set before mayLeave answered`).toMatch(/\bmayLeave\(/);
      } else expect(OPEN_ID[key].length).toBeGreaterThan(10);
    }
    expect(seen.sort(), "a new way out sets openId: ask mayLeave first, or list it with its reason").toEqual(Object.keys(OPEN_ID).sort());

    const LIBRARY_CHANGED: Record<string, string> = {
      onMount: "onSettingsChanged: the folder was switched outside this window, nothing can ask any more",
      "markup:onanswered": "an AI's folder request allowed: FolderRequest asked mayLeave before the switch",
      "markup:onlibrarychanged": "the Settings folder field: SettingsDialog asked mayLeave before the switch",
    };
    const uses: string[] = [];
    for (const m of app.matchAll(/\blibraryChanged\b/g)) {
      if (/function\s+$/.test(app.slice(Math.max(0, m.index! - 20), m.index!))) continue;
      const w = where(m.index!);
      uses.push(typeof w === "string" ? w : w.name);
    }
    expect(uses.sort(), "a new caller of libraryChanged: it closes the editor, so ask mayLeave before the switch, or list it with its reason").toEqual(Object.keys(LIBRARY_CHANGED).sort());
  });
});

/** The body of `name` in `src`, from its declaration to its closing line. */
function body2(src: string, name: string): string {
  const at = src.indexOf(`function ${name}(`);
  if (at < 0) return "";
  return src.slice(at, src.indexOf("\n  }\n", at));
}

describe("round 14: the press's questions and exits (TEST-1303, TEST-1305, UX-1301)", () => {
  const script = (src: string) => {
    const at = src.indexOf('<script lang="ts">');
    return src.slice(at, src.indexOf("</script>", at));
  };

  /** Every named function of `src` with the range of its body. */
  function functions(src: string): { name: string; from: number; to: number }[] {
    const out: { name: string; from: number; to: number }[] = [];
    for (const m of src.matchAll(/\bfunction\s+([\w$]+)\s*(?:<[^>(]*>)?\(/g)) {
      const params = balanced(src, m.index! + m[0].length - 1);
      const open = src.indexOf("{", m.index! + m[0].length - 1 + params.length);
      out.push({ name: m[1], from: open, to: open + balanced(src, open).length });
    }
    return out;
  }

  it("asking() counts its question down in a finally, so a throw cannot leave the claim held (TEST-1303)", () => {
    const editor = stripComments(files["Editor.svelte"]);
    const at = editor.indexOf("async function asking<R>(");
    expect(at).toBeGreaterThan(0);
    const asking = editor.slice(at, editor.indexOf("\n  }\n", at));
    expect(asking).toMatch(/questions\+\+;\s*try \{[\s\S]*\} finally \{\s*questions--/);
  });

  it("every confirm( call in app/src is awaited right there, so none slips past the scan below as .then (TEST-1403)", () => {
    const calls: string[] = [];
    const bare: string[] = [];
    for (const [file, src] of Object.entries(files)) {
      const code = stripComments(src);
      // A call of confirm or window.confirm, not a method of another object or a definition.
      for (const m of code.matchAll(/(?<![\w$.])(?:window\.)?confirm\s*\(/g)) {
        const before = code.slice(Math.max(0, m.index! - 40), m.index!);
        if (/function\s+$/.test(before)) continue;
        calls.push(file);
        if (!/\bawait\s+$/.test(before)) bare.push(`${file}: …${before.slice(-30)}${m[0]}`);
      }
    }
    expect(calls.length, "the scan finds the confirm calls").toBeGreaterThan(5);
    expect(bare, "await confirm(…) directly: a Promise is always truthy, and confirm(…).then edits escape the asking() scan").toEqual([]);
  });

  it("the confirm scan flags a call that is not awaited (TEST-1403)", () => {
    const bad = ["if (confirm(t(\"x\"))) del();", "confirm(t(\"x\")).then((ok) => ok && del());", "void window.confirm(\"x\");"];
    for (const code of bad) {
      const m = [...code.matchAll(/(?<![\w$.])(?:window\.)?confirm\s*\(/g)][0];
      const before = code.slice(0, m.index!);
      expect(/\bawait\s+$/.test(before), code).toBe(false);
    }
    const good = "if (!(await confirm(t(\"x\")))) return;";
    const m = [...good.matchAll(/(?<![\w$.])(?:window\.)?confirm\s*\(/g)][0];
    expect(/\bawait\s+$/.test(good.slice(0, m.index!))).toBe(true);
  });

  it("every await confirm( of the editor is inside asking() or listed as passive with a reason (TEST-1305)", () => {
    const editor = script(stripComments(files["Editor.svelte"]));
    const fns = functions(editor);
    const ASKING = ["deleteVertexByKey", "deleteDormer", "clearRoof", "deletePart"];
    const PASSIVE: Record<string, string> = {
      leaveGuard: "the way out asks before dropping a footprint being drawn; no job edit follows its answer",
      revertNow: "asks before Revert all; the press and its prepare phase start only after the answer",
      cancelDraw: "drops the footprint being drawn, which is not in the job; no job edit follows",
    };
    const seen: string[] = [];
    for (const m of editor.matchAll(/await\s+confirm\(/g)) {
      const at = m.index!;
      const inner = fns.filter((f) => f.from < at && at < f.to).sort((a, b) => a.to - a.from - (b.to - b.from))[0];
      expect(inner, `an await confirm( outside any named function at ${at}`).toBeTruthy();
      expect(ASKING.includes(inner.name) || inner.name in PASSIVE, `${inner.name}: a new confirm-then-edit: wrap it in asking() and list it, or list it as passive with why`).toBe(true);
      seen.push(inner.name);
      if (ASKING.includes(inner.name)) {
        const body = editor.slice(inner.from, inner.to);
        const call = body.search(/\basking\(/);
        expect(call, `${inner.name} runs asking(`).toBeGreaterThan(0);
        const span = balanced(body, body.indexOf("(", call));
        const rel = at - inner.from;
        const start = body.indexOf("(", call);
        expect(rel > start && rel < start + span.length, `${inner.name}: the confirm lies inside asking(`).toBe(true);
      }
    }
    expect([...new Set(seen)].sort()).toEqual([...ASKING, ...Object.keys(PASSIVE)].sort());
  });

  it("the prepare phase names what held it at the deadline: busy_open for a menu, dialog or question, busy_edits for edits (UX-1301)", () => {
    const editor = stripComments(files["Editor.svelte"]);
    const prep = body2(editor, "prepareReview");
    expect(prep).toMatch(/const held = questions > 0 \|\| !!menu \|\| openModals\(\) > 0;/);
    expect(prep).toMatch(/if \(settled && held\) notifyError\(\{ code: "busy_open"/);
    expect(prep).toContain('code: "busy_edits"');
  });
});
