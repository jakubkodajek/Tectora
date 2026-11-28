// Audit 2026-11-24 ERR-1902, ERR-1904, ERR-1905, ERR-1906 (fix design 2):
// an answer of a read is applied only if no newer request of the same kind
// was started since. Two file events in a row, an Accept while a reload is
// out, or an AI save while a dialog is open let reads answer out of order;
// the older answer must never replace the newer one. This scan lists every
// call of a read command in app/src: each is either guarded where it is (a
// `sequence()`, `latest()`, the editor's single-flight load or a hand-made
// counter, named by the pattern that must stand next to it) or written down
// as a read inside one user action, with why. A new read fails here until it
// is one or the other.
import { describe, expect, it } from "vitest";

const sources = import.meta.glob(["../**/*.svelte", "../**/*.ts", "!../**/*.test.ts", "!../dev/**"], { query: "?raw", import: "default", eager: true }) as Record<string, string>;
const files = Object.fromEntries(Object.entries(sources).map(([k, v]) => [k.replace(/^\.\.?\//, "").replace(/^lib\//, ""), v]));

/** The read commands of `api.ts` whose answers are shown. */
const READS = [
  "listProjects",
  "listDeleted",
  "listAttachments",
  "projectActivity",
  "reviewStatus",
  "resultRecalculation",
  "loadProject",
  "loadWarnings",
  "exportPreview",
  "calc",
  "calcBreakdown",
  "getSettings",
  "liveStatus",
  "dormerHost",
  "checkDormer",
  "itemPlacement",
  "roofHelperContext",
  "roofHelperPreview",
  "roundArc",
  "faceArea",
  "projectsDir",
  "listExamples",
];

type Entry = ({ guard: RegExp } | { action: string }) & { count?: number };
const ACTION = "awaited inside one user action (a press, a dialog's OK): nothing else shows its answer";

/** `file:command` -> how its answer is kept in order (default count 1). */
const CALLS: Record<string, Entry> = {
  "App.svelte:listProjects": { guard: /listings\.current\(/ },
  "App.svelte:getSettings": { guard: /settingsReads\.current\(/ },
  "App.svelte:liveStatus": { action: "start-up read; `onLive` keeps the newest `ts_ms`, an older status never replaces a newer one" },
  "App.svelte:listExamples": { action: "read once at start-up" },
  "App.svelte:projectsDir": { count: 2, action: "the folder's name, read at start-up and after a folder switch the user or an AI made (settings event)" },
  "settings.svelte.ts:getSettings": { action: "read once at start-up, before any settings event" },
  "Home.svelte:listDeleted": { guard: /trashReads\.current\(/ },
  "JobPanel.svelte:listAttachments": { guard: /fileReads\.current\(/ },
  "ReportDialog.svelte:listAttachments": { action: "read once when the dialog opens, to show the drawings' count and size" },
  "ExportDialog.svelte:exportPreview": { guard: /previews\.current\(/ },
  "exportlist.ts:exportPreview": { action: "the wrapper the Export dialog calls; its call is guarded there" },
  "Editor.svelte:loadProject": { guard: /async function loadOnce/ },
  "Editor.svelte:loadWarnings": { guard: /async function loadOnce/ },
  "Editor.svelte:reviewStatus": { guard: /activityReads\.current\(/ },
  "Editor.svelte:projectActivity": { count: 2, guard: /activityReads\.current\(|restoreOfferResult/ },
  "Editor.svelte:resultRecalculation": { count: 2, guard: /activityReads\.current\(|recalcFor === taken/ },
  "Editor.svelte:calc": { count: 2, guard: /calcSeq|let live = true/ },
  "Editor.svelte:roofHelperContext": { count: 2, guard: /latest\(|async function openRound/ },
  "Editor.svelte:projectsDir": { count: 3, action: "the folder the job was opened from (mount) and the save's check that it is still the same; compared, never shown" },
  "Results.svelte:calcBreakdown": { guard: /mine === seq/ },
  "Breakdown.svelte:calcBreakdown": { guard: /mine !== seq/ },
  "DormerDialog.svelte:dormerHost": { guard: /current|[aA]sked/ },
  "DormerDialog.svelte:itemPlacement": { guard: /current|[aA]sked/ },
  "DormerDialog.svelte:calc": { guard: /n !== aboveAsked/ },
  "DormerDialog.svelte:checkDormer": { action: ACTION },
  "DormerBar.svelte:checkDormer": { action: ACTION },
  "NewJob.svelte:loadProject": { action: ACTION },
  "StandardRoofForm.svelte:calc": { guard: /if \(!current\) return/ },
  "ManualBar.svelte:faceArea": { guard: /latest\(/ },
  "RoofCalcForm.svelte:roofHelperContext": { guard: /latest\(/ },
  "RoofCalcForm.svelte:roofHelperPreview": { guard: /latest\(/ },
  "RoundRoofForm.svelte:roundArc": { guard: /latest\(/ },
};

/** Comments out, line numbers kept. */
const blank = (c: string) => c.replace(/[^\n]/g, "");
const stripComments = (s: string) =>
  s
    .replace(/\/\*[\s\S]*?\*\//g, blank)
    .replace(/<!--[\s\S]*?-->/g, blank)
    .replace(/(^|[^:"'`\\])\/\/[^\n]*/g, "$1");

/** Every read call: `file:command` with the line it is on. */
function readCalls(): { key: string; file: string; line: number }[] {
  const out: { key: string; file: string; line: number }[] = [];
  const names = READS.join("|");
  for (const [file, raw] of Object.entries(files)) {
    if (file === "api.ts") continue;
    const src = stripComments(raw);
    const re = new RegExp(`\\bapi\\s*\\.\\s*(${names})\\s*\\(|(?<![.\\w])(exportPreview)\\s*\\(`, "g");
    for (const m of src.matchAll(re)) {
      const name = m[1] ?? m[2];
      // The wrapper's own definition is no call.
      if (file === "exportlist.ts" && m[2]) continue;
      out.push({ key: `${file}:${name}`, file, line: src.slice(0, m.index).split("\n").length });
    }
  }
  return out;
}

describe("every shown read keeps its answers in order (audit 2026-11-24 ERR-1902..ERR-1906)", () => {
  const calls = readCalls();

  it("finds the reads (the scan reads the code)", () => {
    expect(calls.length).toBeGreaterThan(25);
    expect(calls.some((c) => c.key === "Editor.svelte:reviewStatus")).toBe(true);
  });

  it("lists every read with its guard or as part of one user action", () => {
    const counts = new Map<string, number>();
    for (const c of calls) counts.set(c.key, (counts.get(c.key) ?? 0) + 1);
    const unlisted = [...counts.keys()].filter((k) => !CALLS[k]);
    expect(unlisted, "a read with no guard and no reason: guard it with sequence() / latest() or write down why").toEqual([]);
    const wrongCount = Object.entries(CALLS)
      .filter(([k, e]) => (counts.get(k) ?? 0) !== (e.count ?? 1))
      .map(([k, e]) => `${k}: ${counts.get(k) ?? 0} calls, listed ${e.count ?? 1}`);
    expect(wrongCount).toEqual([]);
  });

  it("has the guard next to each guarded read", () => {
    const bad: string[] = [];
    for (const c of calls) {
      const e = CALLS[c.key];
      if (!e || !("guard" in e)) continue;
      const lines = stripComments(files[c.file]).split("\n");
      const near = lines.slice(Math.max(0, c.line - 41), c.line + 20).join("\n");
      if (!e.guard.test(near)) bad.push(`${c.key} (line ${c.line}): no ${e.guard} near it`);
    }
    expect(bad).toEqual([]);
  });

  it("guards the four reads of the audit with a sequence", () => {
    for (const [file, name] of [
      ["App.svelte", "listings"],
      ["Home.svelte", "trashReads"],
      ["JobPanel.svelte", "fileReads"],
      ["ExportDialog.svelte", "previews"],
      ["Editor.svelte", "activityReads"],
    ]) {
      expect(files[file], file).toMatch(new RegExp(`const ${name} = sequence\\(\\)`));
    }
    // The editor's load runs one at a time and reads once more after it.
    expect(files["Editor.svelte"]).toMatch(/if \(loading\) \{\s*if \(!initial\) loadAgain = true;/);
  });
});
