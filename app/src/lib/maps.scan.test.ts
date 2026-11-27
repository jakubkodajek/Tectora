// Round 14 (ARCH-1301): a plain object used as a set or map with keys from
// data finds Object.prototype's members: `busy["constructor"]` is the
// inherited function, so a job named "Constructor" looked busy for good and
// its Duplicate and Delete stayed disabled. Data-keyed sets and maps are
// `SvelteSet` / `SvelteMap` / `Set` / `Map`, or read with `Object.hasOwn`.
// This scan lists every `Record<string, …>` (or `{ [k: string]: … }`) variable of the Svelte files
// and the `.ts` modules with what its keys are: a new one fails here
// until it is written down with a reason its keys cannot be such a name.
import { describe, expect, it } from "vitest";

// Plain `.ts` modules too (TEST-1403): filerequest.ts read its table with a
// purpose from settings.json.
const sources = import.meta.glob(["./**/*.svelte", "./**/*.ts", "../*.svelte", "../*.ts", "!./**/*.test.ts", "!./**/*.d.ts"], { query: "?raw", import: "default", eager: true }) as Record<string, string>;
const files = Object.fromEntries(Object.entries(sources).map(([k, v]) => [k.replace(/^\.\.?\//, ""), v]));

/** `file:variable` -> what its keys are and why a prototype name cannot hurt. */
const ALLOWED: Record<string, string> = {
  "DormerDialog.svelte:TILE": "constant table, read with the dialog's own fixed keys",
  "Icon.svelte:paths": "constant table, keyed by icon names written in the code",
  "StandardRoofForm.svelte:invalid": "keyed by the form's own field labels",
  "Results.svelte:by": "keyed by the core's opening kinds (a fixed enum)",
  "lineinfo.ts:LINE_TOKENS": "constant table, keyed by the core's line kinds (a fixed enum), read with a `??` fallback",
  "lineinfo.ts:OWN": "constant table, keyed by line codes, read only through `Object.hasOwn` (lineTitle)",
  "PlanView.svelte:lineColor": "constant table, keyed by the core's line kinds (a fixed enum)",
  "settings.svelte.ts:reviewed": "keyed by job id, read only through `Object.hasOwn` (reviewedAt)",
  "settings.svelte.ts:patch": "keyed by the settings' own field names",
  "AiPanel.svelte:open": "keyed by change-log timestamps (digits)",
  "Home.svelte:c": "keyed by `all`, `ai` and the core's job statuses (a fixed enum)",
  // Plain .ts modules (TEST-1403).
  "filerequest.ts:PURPOSES": "keyed by a purpose from settings.json, read only through `Object.hasOwn` (fileRequestPurpose)",
  "linecodes.ts:KEY": "keyed by line codes, also a hand-drawn line's code from the job file: read only through `Object.hasOwn` (lineName)",
  "linecodes.ts:SUB_CODES": "constant table, read with the Results rows' own codes (O, W) and a `??` fallback",
  "manual.ts:codeKind": "keyed by a hand-drawn line's code from the job file: module-private, read only through `Object.hasOwn` (kindOf)",
  "changes.ts:ITEM_LABEL": "keyed by the change list's field names, read only after `hasOwnProperty` (isItemChange)",
  "fold.ts:SPELLED": "constant table, keyed by one character of a name (no prototype member has a one-character name)",
  "tabs.ts:NEXT": "constant table, keyed by KeyboardEvent.key (arrow keys; no key value is a prototype member's name)",
  "merge.ts:out": "built from a job's own keys, written only, `__proto__` skipped (ARCH-1202)",
};

/**
 * Every `const|let|var name: Record<string, …>`, `name: { [k: string]: … }`
 * and `name = $state<Record<string, …>>` (or the index signature there).
 */
function records(): string[] {
  const found: string[] = [];
  const re = /\b(?:const|let|var)\s+(\w+)\s*(?::\s*(?:Record<string,|\{\s*\[\w+\s*:\s*string\s*\]\s*:)|=\s*\$state<(?:Record<string,|\{\s*\[\w+\s*:\s*string\s*\]\s*:))/g;
  for (const [file, src] of Object.entries(files)) {
    for (const m of src.matchAll(re)) found.push(`${file}:${m[1]}`);
  }
  return found.sort();
}

describe("plain objects keyed by data (ARCH-1301)", () => {
  it("finds the Svelte files", () => {
    expect(Object.keys(files)).toContain("Home.svelte");
    expect(Object.keys(files)).toContain("settings.svelte.ts");
  });

  it("lists every Record<string, …> variable of the Svelte files with what its keys are", () => {
    const unknown = records().filter((r) => !(r in ALLOWED));
    expect(unknown, "use a Set / Map (SvelteSet / SvelteMap) for keys from data, or write the variable down here with why its keys are safe").toEqual([]);
  });

  it("has no stale entry", () => {
    const found = new Set(records());
    expect(Object.keys(ALLOWED).filter((k) => !found.has(k))).toEqual([]);
  });

  it("also finds the index-signature form and plain .ts modules (TEST-1403)", () => {
    expect(Object.keys(files)).toContain("filerequest.ts");
    expect(records()).toContain("filerequest.ts:PURPOSES");
    const re = /\b(?:const|let|var)\s+(\w+)\s*(?::\s*(?:Record<string,|\{\s*\[\w+\s*:\s*string\s*\]\s*:)|=\s*\$state<(?:Record<string,|\{\s*\[\w+\s*:\s*string\s*\]\s*:))/g;
    const sample = "const a: { [k: string]: number } = {};\nlet b = $state<{ [id: string]: boolean }>({});\nconst c: Record<string, string> = {};";
    expect([...sample.matchAll(re)].map((m) => m[1])).toEqual(["a", "b", "c"]);
  });

  it("the data-keyed tables of the .ts modules are read through Object.hasOwn (TEST-1403)", () => {
    expect(files["filerequest.ts"]).toMatch(/Object\.hasOwn\(PURPOSES, purpose\)/);
    expect(files["linecodes.ts"]).toMatch(/Object\.hasOwn\(KEY, code\)/);
    expect(files["manual.ts"]).toMatch(/Object\.hasOwn\(codeKind, code\)/);
    expect(files["manual.ts"]).not.toMatch(/export const codeKind/);
  });

  it("Home's busy marks and JobPanel's removal marks are sets", () => {
    expect(files["Home.svelte"]).toMatch(/const busy = new SvelteSet<string>\(\)/);
    expect(files["JobPanel.svelte"]).toMatch(/const removing = new SvelteSet<string>\(\)/);
    expect(files["Home.svelte"]).toMatch(/const deleteToasts = new Map<string, number>\(\)/);
  });

  it("an own-key read stays false for prototype names", () => {
    const plain: Record<string, number> = {};
    for (const name of ["constructor", "toString", "valueOf", "hasOwnProperty"]) {
      expect(Object.hasOwn(plain, name)).toBe(false);
      expect(new Set<string>().has(name)).toBe(false);
    }
  });
});
