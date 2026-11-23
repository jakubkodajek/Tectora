// Round 14 (ARCH-1301): a plain object used as a set or map with keys from
// data finds Object.prototype's members: `busy["constructor"]` is the
// inherited function, so a job named "Constructor" looked busy for good and
// its Duplicate and Delete stayed disabled. Data-keyed sets and maps are
// `SvelteSet` / `SvelteMap` / `Set` / `Map`, or read with `Object.hasOwn`.
// This scan lists every `Record<string, …>` variable of the Svelte files
// (and `.svelte.ts` modules) with what its keys are: a new one fails here
// until it is written down with a reason its keys cannot be such a name.
import { describe, expect, it } from "vitest";

const sources = import.meta.glob(["./**/*.svelte", "./**/*.svelte.ts", "../*.svelte", "!./**/*.test.ts"], { query: "?raw", import: "default", eager: true }) as Record<string, string>;
const files = Object.fromEntries(Object.entries(sources).map(([k, v]) => [k.replace(/^\.\.?\//, ""), v]));

/** `file:variable` -> what its keys are and why a prototype name cannot hurt. */
const ALLOWED: Record<string, string> = {
  "DormerDialog.svelte:TILE": "constant table, read with the dialog's own fixed keys",
  "Icon.svelte:paths": "constant table, keyed by icon names written in the code",
  "StandardRoofForm.svelte:invalid": "keyed by the form's own field labels",
  "Results.svelte:by": "keyed by the core's opening kinds (a fixed enum)",
  "Roof3D.svelte:LINE_TOKENS": "constant table, keyed by the core's line kinds (a fixed enum), read with a `??` fallback",
  "PlanView.svelte:lineColor": "constant table, keyed by the core's line kinds (a fixed enum)",
  "settings.svelte.ts:reviewed": "keyed by job id, read only through `Object.hasOwn` (reviewedAt)",
  "settings.svelte.ts:patch": "keyed by the settings' own field names",
  "AiPanel.svelte:open": "keyed by change-log timestamps (digits)",
  "Home.svelte:c": "keyed by `all`, `ai` and the core's job statuses (a fixed enum)",
};

/** Every `const|let|var name: Record<string, …>` and `name = $state<Record<string, …>>`. */
function records(): string[] {
  const found: string[] = [];
  const re = /\b(?:const|let|var)\s+(\w+)\s*(?::\s*Record<string,|=\s*\$state<Record<string,)/g;
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
