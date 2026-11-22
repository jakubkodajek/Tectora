import { describe, expect, it } from "vitest";
import { DICTS, i18n } from "./i18n.svelte";
import { actorLabel, entrySummary, isRevert, liveText, refreshedNotice } from "./live.svelte";

// The codes the MCP server sends (crates/tectora-core/src/store.rs LIVE_KINDS).
const KINDS = [
  "settings",
  "browsing",
  "reading",
  "calculating",
  "explaining",
  "comparing",
  "showing",
  "reverted",
  "reverted_changes",
  "copied",
  "deleted",
  "restored",
  "attached",
  "removed",
  "exported",
  "exported_sheet",
  "imported",
];

const status = (kind: string | undefined, message: string, name?: string) => ({ ts_ms: 0, actor: "claude", message, project_id: "a", kind, name });

describe("liveText (UX-002)", () => {
  it("has every status sentence in all four languages", () => {
    for (const locale of Object.keys(DICTS) as (keyof typeof DICTS)[]) {
      for (const k of KINDS) expect(DICTS[locale][`ai.live.${k}`], `${locale} ${k}`).toBeTruthy();
    }
  });

  it("translates the code and keeps the agent's own summary for a change", () => {
    i18n.locale = "cs";
    expect(liveText(status("reading", "Reading “Sedlo”", "Sedlo"))).toBe("Čte „Sedlo“");
    expect(liveText(status("changed", "Sklon 42° podle řezu"))).toBe("Sklon 42° podle řezu");
    // An older server without a code, or a code this app does not know.
    expect(liveText(status(undefined, "Reading “Sedlo”"))).toBe("Reading “Sedlo”");
    expect(liveText(status("dancing", "Dancing"))).toBe("Dancing");
  });
});

describe("refreshedNotice", () => {
  // Audit UX-702c: the notice on open says what changed in every language.
  it("names the engines and the old and new roof area in every language", () => {
    const entry = "Stored result recalculated (Tectora 0.8.0 -> 0.9.0): roof area 150 -> 129.24 m2";
    for (const locale of Object.keys(DICTS) as (keyof typeof DICTS)[]) {
      i18n.locale = locale;
      const text = refreshedNotice(entry);
      expect(text, locale).not.toContain("{");
      expect(text, locale).toContain("0.8.0 → 0.9.0");
    }
    i18n.locale = "cs";
    expect(refreshedNotice(entry)).toContain("150,00 → 129,24 m²");
    expect(refreshedNotice("Something else")).toBe("Something else");
  });
});

describe("entrySummary", () => {
  it("translates the entry of a stored result the corrected calculation recalculated", () => {
    const entry = "Stored result recalculated (Tectora 0.9.0 -> 0.9.1): roof area 184.72 -> 180.8 m2";
    for (const locale of Object.keys(DICTS) as (keyof typeof DICTS)[]) {
      i18n.locale = locale;
      const text = entrySummary(entry);
      expect(text, locale).not.toContain("{");
      expect(text, locale).not.toContain(" -> ");
      expect(text, locale).toContain("0.9.0 → 0.9.1");
    }
    i18n.locale = "cs";
    expect(entrySummary(entry)).toContain("184,72 → 180,80 m²");
  });

  // The summary of a real recalculation, written by the core test
  // store::tests::the_recalculation_summary_is_the_one_the_app_translates
  // (TEST-711): a change of the core's wording fails here.
  it("translates the entry of a real recalculation the core wrote, in every language", () => {
    const files = import.meta.glob("../../../tests/fixtures/parity/refreshed_entry.json", { eager: true, import: "default" }) as Record<string, unknown>;
    const file = Object.values(files)[0] as { from: string; to: string; summary: string };
    expect(file.summary).toMatch(/^Stored result recalculated/);
    // The two areas: the last two numbers of the summary.
    const [a, b] = (file.summary.match(/\d+\.\d+/g) ?? []).slice(-2).map(Number);
    expect(Number.isFinite(a) && Number.isFinite(b)).toBe(true);
    for (const locale of Object.keys(DICTS) as (keyof typeof DICTS)[]) {
      i18n.locale = locale;
      const text = entrySummary(file.summary);
      expect(text, locale).not.toBe(file.summary);
      expect(text, locale).not.toContain("{");
      expect(text, locale).toContain(`${file.from} → ${file.to}`);
      const n = (x: number) => x.toLocaleString(locale, { minimumFractionDigits: 2, maximumFractionDigits: 2 });
      expect(text, locale).toContain(`${n(a)} → ${n(b)} m²`);
    }
    i18n.locale = "cs";
  });

  it("translates the revert entries the core writes, nested too", () => {
    i18n.locale = "cs";
    expect(entrySummary("Reverted: Sklon 45°")).toBe("Vráceno: Sklon 45°");
    expect(entrySummary("Reverted 2 changes since: Sklon 45°")).toBe("Vráceno změn: 2, od: Sklon 45°");
    expect(entrySummary("Reverted: Reverted: X")).toBe("Vráceno: Vráceno: X");
    expect(entrySummary("Sklon 45°")).toBe("Sklon 45°");
    expect(isRevert("Reverted 3 changes since: x") && isRevert("Reverted: x") && !isRevert("Revert the eave")).toBe(true);
    i18n.locale = "de";
    expect(entrySummary("Reverted: Pitch")).toBe("Verworfen: Pitch");
  });

  it("translates a revert with the AI's reason, in every language (UX-002)", () => {
    const want: Record<string, string> = {
      cs: "Vráceno: na část (důvod: zpět)",
      sk: "Vrátené: na část (dôvod: zpět)",
      en: "Reverted: na část (reason: zpět)",
      de: "Verworfen: na část (Grund: zpět)",
    };
    for (const [loc, text] of Object.entries(want)) {
      i18n.locale = loc as typeof i18n.locale;
      expect(entrySummary("Reverted: na část (reason: zpět)")).toBe(text);
    }
    i18n.locale = "cs";
    expect(entrySummary("Reverted 2 changes since: X (reason: back)")).toBe("Vráceno změn: 2, od: X (důvod: back)");
    // A revert of a revert with reasons: the outer reason is the last one.
    expect(entrySummary("Reverted: Reverted: X (reason: a) (reason: b)")).toBe("Vráceno: Vráceno: X (důvod: a) (důvod: b)");
    // Entries written before the fixed form: `reason (Reverted: …)`.
    expect(entrySummary("undo (Reverted: to part)")).toBe("Vráceno: to part (důvod: undo)");
    expect(isRevert("undo (Reverted: to part)")).toBe(true);
    // An AI summary that only mentions a reason is left as it is.
    expect(entrySummary("Pitch 45 (reason: drawing)")).toBe("Pitch 45 (reason: drawing)");
  });
});

// Audit 2026-11-22 UX-704: the library's job panel showed the engine's
// recalculation of a stored result as the user's own change.
describe("actorLabel", () => {
  it("names the AI, the engine and the user apart, in every language", () => {
    const you: Record<string, string> = {};
    for (const locale of Object.keys(DICTS) as (keyof typeof DICTS)[]) {
      i18n.locale = locale;
      you[locale] = actorLabel("user");
      expect(actorLabel("tectora"), locale).toBe("Tectora");
      expect(actorLabel("claude-ai"), locale).toBe("Claude");
      expect(actorLabel("tectora"), locale).not.toBe(you[locale]);
    }
    expect(you.cs).toBe("Vy");
    i18n.locale = "cs";
  });
});
