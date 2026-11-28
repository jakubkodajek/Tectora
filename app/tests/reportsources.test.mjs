// The reports folder is gone from the UI sources (plan-delivery D8, D17,
// ADR 0008): no settings row, no folder request key, no type, no preview
// stand-in. Plain Node so it can read the sources without Node typings in
// the Svelte type check.
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const lib = join(dirname(fileURLToPath(import.meta.url)), "..", "src", "lib");
const read = (f) => readFileSync(join(lib, f), "utf8");

describe("the reports folder is gone from the UI", () => {
  for (const f of ["SettingsDialog.svelte", "FolderRequest.svelte", "types.ts", "api.ts", "ReportDialog.svelte", "settings.svelte.ts"]) {
    it(`${f} no longer mentions it`, () => {
      const src = read(f);
      expect(src).not.toMatch(/reports_dir|reportsDir/);
    });
  }

  it("the folder request knows two keys", () => {
    expect(read("FolderRequest.svelte")).toMatch(/\["projects_dir", "drawings_dir"\] as const/);
  });

  it("the dialog does not read a folder setting or ask for 'reports' to be opened", () => {
    const src = read("ReportDialog.svelte");
    expect(src).not.toMatch(/openFolder\(["']reports["']\)/);
    expect(src).not.toContain("report.destMissing");
  });

  it("api.ts has the outbox calls the commands need", () => {
    const api = read("api.ts");
    for (const c of ["bug_report_outbox", "send_bug_reports", "save_queued_bug_report", "discard_bug_report"]) {
      expect(api, c).toContain(c);
    }
  });

  it("the outbox list exists and asks before it discards", () => {
    const src = read("ReportOutbox.svelte");
    expect(src).toMatch(/await\s+(window\.)?confirm\(/);
    expect(src).toContain("discardBugReport");
  });
});
