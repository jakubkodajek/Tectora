// Round 12 (audit 2026-11-23 fix design 1; HUNT-1101, ERR-1101, UI-1101,
// HUNT-1103, ERR-1105, TEST-1105): while a writer of the job is out (a drag,
// Revert, Revert all, the offer put-back) the editor takes no edit. Its own
// refusal lives in its DOM (inert, refuseInput, its key handler); anything at
// window level, any toast action and any way out of the editor passes it
// unless it asks the module gate (dragsession.ts `anyWriterOpen`,
// `gatedKeys`). This scan lists every such entry point in app/src: a new
// one fails here until it is gated or written down as passive with a reason.
import { describe, expect, it } from "vitest";

const sources = import.meta.glob(["../**/*.svelte", "../**/*.ts", "!../**/*.test.ts", "!../dev/**"], { query: "?raw", import: "default", eager: true }) as Record<string, string>;
const files = Object.fromEntries(Object.entries(sources).map(([k, v]) => [k.replace(/^\.\.?\//, "").replace(/^lib\//, ""), v]));

/** Window, document and body listeners: `gated` (the file asks the gate) or `passive` (writes nothing), with why. */
const LISTENERS: Record<string, { gated: true } | { passive: string }> = {
  "App.svelte:keydown": { gated: true },
  "App.svelte:beforeunload": { passive: "browser preview only: warns about unsaved edits, writes nothing" },
  "App.svelte:error": { passive: "shows an unexpected error, writes nothing" },
  "App.svelte:unhandledrejection": { passive: "shows an unexpected error, writes nothing" },
  "Editor.svelte:keydown": { gated: true },
  "DrawBar.svelte:keydown": { gated: true },
  "Modal.svelte:keydown": { gated: true },
  "ContextMenu.svelte:keydowncapture": { gated: true },
  "ContextMenu.svelte:pointerdowncapture": { passive: "closes the menu only" },
  "ContextMenu.svelte:blur": { passive: "closes the menu only" },
  "ContextMenu.svelte:resize": { passive: "closes the menu only" },
  "ContextMenu.svelte:wheel": { passive: "closes the menu only" },
  "commitfocus.ts:pointerdown": { gated: true },
  "numeric.svelte.ts:pointerdown": { gated: true },
  "itemstep.ts:keydown": { gated: true },
  "dblclick.ts:pointerdown": { passive: "remembers where the last press was, writes nothing" },
  "Home.svelte:focus": { passive: "the library screen, not mounted with the editor" },
  "ProjectCard.svelte:click": { passive: "the library screen, not mounted with the editor; closes a card menu" },
};

/** What calls the gate (the editor's own predicate counts for the editor). */
const GATE = /\b(anyWriterOpen|gatedKeys|inputClosed)\(/;

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

function listeners(): string[] {
  const found = new Set<string>();
  for (const [file, src] of Object.entries(files)) {
    for (const m of src.matchAll(/<svelte:(window|document|body)\b/g)) {
      const tag = tagAt(src, m.index!);
      // `onkeydown={…}` or the shorthand `{onkeydown}`.
      for (const a of tag.matchAll(/(?:\s\{on([a-z]+)\}|\son([a-z]+)=)/g)) found.add(`${file}:${a[1] ?? a[2]}`);
    }
    for (const m of src.matchAll(/\b(?:window|document|win)\.addEventListener\(\s*"(\w+)"/g)) found.add(`${file}:${m[1]}`);
  }
  return [...found].sort();
}

describe("the writer gate covers every entry point outside the editor's refusal (round 12, fix design 1)", () => {
  it("every window / document / body listener is listed, and the gated ones ask the gate", () => {
    const seen = listeners();
    expect(seen.length).toBeGreaterThan(10);
    const unknown = seen.filter((k) => !(k in LISTENERS));
    expect(unknown, "a new window-level listener: gate it (anyWriterOpen / gatedKeys) or list it as passive with a reason").toEqual([]);
    for (const [key, how] of Object.entries(LISTENERS)) {
      expect(seen, `listed but not found: ${key}`).toContain(key);
      const file = key.slice(0, key.lastIndexOf(":"));
      if ("gated" in how) expect(GATE.test(files[file]), `${file} does not ask the gate`).toBe(true);
      else expect(how.passive.length).toBeGreaterThan(5);
    }
  });

  it("every toast action is listed, and they all run through the gated press of Toasts.svelte", () => {
    const actions: string[] = [];
    for (const [file, src] of Object.entries(files)) {
      for (const m of src.matchAll(/action: \{\s*label: t\("([\w.]+)"/g)) actions.push(`${file}:${m[1]}`);
      // Any other shape of a toast action would escape this list.
      const all = [...src.matchAll(/\baction: \{/g)].length;
      const named = [...src.matchAll(/action: \{\s*label: t\("/g)].length;
      expect(all, `${file}: a toast action of another shape`).toBe(named);
    }
    expect(actions.sort()).toEqual(["App.svelte:ai.followOpen", "Editor.svelte:snap.refreshedShow", "Home.svelte:home.restore", "JobPanel.svelte:snap.store"]);
    const toasts = files["Toasts.svelte"];
    expect(toasts).toMatch(/function runAction\([\s\S]*?if \(anyWriterOpen\(\)\)/);
    expect(toasts.match(/action\.run/g)).toEqual(["action.run"]);
    expect(toasts).toContain("runAction(toast.id, action.run)");
  });

  it("the editor's commit and undo step refuse a write while a core action is out", () => {
    const editor = files["Editor.svelte"];
    const body = (name: string) => {
      const at = editor.indexOf(`  function ${name}(`);
      expect(at, name).toBeGreaterThan(0);
      return editor.slice(at, editor.indexOf("\n  }\n", at));
    };
    expect(body("commit")).toContain('reviewing.foreign("commit")');
    expect(body("restore")).toContain('reviewing.foreign("undo step")');
    expect(editor).toMatch(/new CoreAction\([\s\S]*?import\.meta\.env\.DEV,?\s*\)/);
  });

  it("every way out of the editor is listed and waits for a core action (leaveGuard)", () => {
    const sites: string[] = [];
    for (const [file, src] of Object.entries(files)) {
      if (file === "leave.ts") continue;
      const n = [...src.matchAll(/\bmayLeave\(/g)].length;
      if (n) sites.push(`${file}:${n}`);
    }
    // App: follow mode, openJob (Back, follow "Open", a file opened from the system), the window's close; Settings: the folder switch.
    expect(sites.sort()).toEqual(["App.svelte:3", "SettingsDialog.svelte:1"]);
    const editor = files["Editor.svelte"];
    const guard = editor.slice(editor.indexOf("async function leaveGuard("), editor.indexOf("setLeaveGuard(leaveGuard"));
    expect(guard).toMatch(/if \(reviewing\.active\) \{\s*if \(mode === "quiet"\) return false;\s*await reviewing\.settled\(\);/);
  });
});
