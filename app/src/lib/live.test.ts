import { describe, expect, it } from "vitest";
import { DICTS, i18n } from "./i18n.svelte";
import { actorLabel, entrySummary, historyText, isRevert, liveText, refreshedNotice } from "./live.svelte";

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

  it("takes the AI's reason from the entry's own field, whatever its texts contain (HUNT-904)", () => {
    const want: Record<string, [string, string]> = {
      cs: ["Původní čísla nabídky vrácena (Tectora 0.9.0 → 0.0.1) (důvod: Signed (reason: contract) these)", "Vráceno: Note (reason: x) (důvod: Mistake (reason: typo) fixed)"],
      sk: ["", "Vrátené: Note (reason: x) (dôvod: Mistake (reason: typo) fixed)"],
      en: ["Offer figures put back (Tectora 0.9.0 → 0.0.1) (reason: Signed (reason: contract) these)", "Reverted: Note (reason: x) (reason: Mistake (reason: typo) fixed)"],
      de: ["", "Verworfen: Note (reason: x) (Grund: Mistake (reason: typo) fixed)"],
    };
    const restored = "Offer result restored (Tectora 0.9.0 -> 0.0.1): roof area 152.73 -> 157.73 m2 (reason: Signed (reason: contract) these)";
    const reverted = "Reverted: Note (reason: x) (reason: Mistake (reason: typo) fixed)";
    for (const [loc, [r, v]] of Object.entries(want)) {
      i18n.locale = loc as typeof i18n.locale;
      const put = entrySummary(restored, "Signed (reason: contract) these");
      if (r) expect(put).toBe(r);
      expect(put, loc).not.toMatch(/^Offer result restored/);
      expect(put, loc).toContain("Signed (reason: contract) these");
      expect(entrySummary(reverted, "Mistake (reason: typo) fixed")).toBe(v);
      // The history passes the item's field.
      expect(historyText({ actor: "claude", summary: reverted, reason: "Mistake (reason: typo) fixed" }, [])).toBe(v);
    }
    i18n.locale = "cs";
  });
});

// Audit 2026-11-23 HUNT-902: the offer's figures no longer live in the
// change log, so the entry no longer promises they "stay in this
// history"; the recalculation the stored result still is says they can be
// put back, an older or a received one says nothing about it.
describe("the recalculation's entry in the history (HUNT-902)", () => {
  const entry = "Stored result recalculated (Tectora 0.0.1 -> 0.9.0): roof area 157.73 -> 152.73 m2";
  it("says the figures can be put back only on the current recalculation, in every language", () => {
    const back: Record<string, string> = {
      cs: "Původní čísla lze vrátit.",
      sk: "Pôvodné čísla sa dajú vrátiť.",
      en: "The original figures can be put back.",
      de: "Die ursprünglichen Zahlen lassen sich zurückholen.",
    };
    for (const [loc, sentence] of Object.entries(back)) {
      i18n.locale = loc as typeof i18n.locale;
      const current = historyText({ actor: "tectora", summary: entry, ts_ms: 5 }, [], 5);
      expect(current, loc).toMatch(new RegExp(`m²\\. ${sentence.replace(/[.]/g, "\\.")}$`));
      for (const text of [
        historyText({ actor: "tectora", summary: entry, ts_ms: 4 }, [], 5),
        historyText({ actor: "tectora", summary: entry, ts_ms: 5 }, [], null),
        historyText({ actor: "tectora (received)", summary: entry, ts_ms: 5 }, [], 5),
        entrySummary(entry),
      ]) {
        expect(text, loc).not.toContain(sentence);
        expect(text, loc).toMatch(/m²\.$/);
      }
      expect(current, loc).not.toMatch(/histor|Verlauf/i);
    }
    i18n.locale = "cs";
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

// Audit 2026-11-22 UX-807: a recalculation that leaves the roof area said
// "roof area 142.83 -> 142.83 m2", which reads as "nothing changed".
describe("a recalculation that changed no roof area (UX-807)", () => {
  const LOCALES = Object.keys(DICTS) as (keyof typeof DICTS)[];
  const files = import.meta.glob("../../../tests/fixtures/parity/refreshed_more_entries.json", { eager: true, import: "default" }) as Record<string, unknown>;
  // Written by store::tests::a_recalculation_of_line_lengths_only_names_the_line_codes.
  const file = Object.values(files)[0] as { from: string; to: string; codes: string; lines_summary: string; restored_summary: string };

  it("names the line codes of a real recalculation the core wrote, in the notice and the entry, in every language", () => {
    expect(file.lines_summary).toMatch(/^Stored result recalculated .*: line lengths /);
    for (const locale of LOCALES) {
      i18n.locale = locale;
      for (const text of [refreshedNotice(file.lines_summary), entrySummary(file.lines_summary)]) {
        expect(text, locale).not.toBe(file.lines_summary);
        expect(text, locale).not.toContain("{");
        expect(text, locale).not.toContain(" -> ");
        expect(text, locale).toContain(`${file.from} → ${file.to}`);
        expect(text, locale).toContain(`: ${file.codes}.`);
        // No area pair that reads "a → a m²".
        expect(text, locale).not.toContain(" m²");
      }
    }
    i18n.locale = "en";
    expect(refreshedNotice("Stored result recalculated (Tectora 0.8.0 -> 0.9.0): line lengths G, K")).toBe(
      "The offer's stored result was recalculated with the corrected calculation (Tectora 0.8.0 → 0.9.0): the roof area is the same, line lengths changed: G, K.",
    );
    i18n.locale = "cs";
  });

  it("says that other figures changed when neither the area nor a line did", () => {
    const entry = "Stored result recalculated (Tectora 0.8.0 -> 0.9.0): other figures";
    for (const locale of LOCALES) {
      i18n.locale = locale;
      for (const text of [refreshedNotice(entry), entrySummary(entry)]) {
        expect(text, locale).not.toBe(entry);
        expect(text, locale).not.toContain("{");
        expect(text, locale).toContain("0.8.0 → 0.9.0");
      }
    }
    i18n.locale = "cs";
  });

  it("translates the entry of the offer's result put back, with an AI's reason too", () => {
    expect(file.restored_summary).toMatch(/^Offer result restored \(Tectora /);
    for (const locale of LOCALES) {
      i18n.locale = locale;
      const text = entrySummary(file.restored_summary);
      expect(text, locale).not.toBe(file.restored_summary);
      expect(text, locale).not.toContain("{");
      expect(text, locale).toContain(`${file.to} → ${file.from}`);
    }
    i18n.locale = "en";
    expect(entrySummary("Offer result restored (Tectora 0.9.0 -> 0.8.0): roof area 142.83 -> 150.00 m2")).toBe("Offer figures put back (Tectora 0.9.0 → 0.8.0)");
    expect(entrySummary("Offer result restored (Tectora 0.9.0 -> 0.8.0) (reason: The customer signed these)")).toBe(
      "Offer figures put back (Tectora 0.9.0 → 0.8.0) (reason: The customer signed these)",
    );
    // An AI summary that only starts like it is left as it is.
    expect(entrySummary("Offer result restored by hand")).toBe("Offer result restored by hand");
    i18n.locale = "cs";
  });
});

// Audit 2026-11-22 UI-803: the user's revert of Tectora's recalculation was
// listed as "AI changes reverted".
describe("historyText (UI-803)", () => {
  const RECALC = "Stored result recalculated (Tectora 0.8.0 -> 0.9.0): roof area 150.00 -> 142.83 m2";
  const log = [
    { ts_ms: 30, actor: "user" },
    { ts_ms: 20, actor: "claude" },
    { ts_ms: 10, actor: "tectora" },
  ];

  it("says 'AI changes reverted' only when the undone entry is an AI's, in every language", () => {
    for (const locale of Object.keys(DICTS) as (keyof typeof DICTS)[]) {
      i18n.locale = locale;
      const ai = DICTS[locale]["ai.revertedEntry"];
      // The user's revert of the AI's entry.
      expect(historyText({ actor: "user", summary: "Reverted: Notes", undid_from: 20 }, log), locale).toBe(ai);
      // Of Tectora's recalculation: the sentence itself, no AI in it.
      const engine = historyText({ actor: "user", summary: `Reverted: ${RECALC}`, undid_from: 10 }, log);
      expect(engine, locale).not.toBe(ai);
      expect(engine, locale).toBe(entrySummary(`Reverted: ${RECALC}`));
      expect(engine, locale).toContain("0.8.0 → 0.9.0");
      // The undone entry is no longer in the list: told by the sentence.
      expect(historyText({ actor: "user", summary: `Reverted: ${RECALC}`, undid_from: 5 }, log), locale).not.toBe(ai);
      expect(historyText({ actor: "user", summary: `Reverted: ${RECALC}` }, log), locale).not.toBe(ai);
      expect(historyText({ actor: "user", summary: "Reverted: Notes" }, log), locale).toBe(ai);
      // The user's revert of the user's own logged change is no AI change either.
      expect(historyText({ actor: "user", summary: "Reverted: Mine", undid_from: 30 }, log), locale).toBe(entrySummary("Reverted: Mine"));
    }
    i18n.locale = "cs";
  });

  it("shows a restore of the offer's figures and an AI's own revert as their sentences", () => {
    i18n.locale = "en";
    expect(historyText({ actor: "user", summary: "Offer result restored (Tectora 0.9.0 -> 0.8.0)", restores: 10 }, log)).toBe("Offer figures put back (Tectora 0.9.0 → 0.8.0)");
    expect(historyText({ actor: "claude", summary: "Reverted: Notes (reason: wrong)", undid_from: 20 }, log)).toBe("Reverted: Notes (reason: wrong)");
    expect(historyText({ actor: "claude", summary: "Pitch 45" }, log)).toBe("Pitch 45");
    i18n.locale = "cs";
  });
});

// Audit 2026-11-22 ARCH-802: an engine's entry that came with a bundle is
// neither this Tectora's nor an AI's.
describe("actorLabel of a received engine entry (ARCH-802)", () => {
  it("is named as received, never as Tectora or as the user, in every language", () => {
    for (const locale of Object.keys(DICTS) as (keyof typeof DICTS)[]) {
      i18n.locale = locale;
      const label = actorLabel("tectora (received)");
      expect(label, locale).toBe(DICTS[locale]["ai.receivedEngine"]);
      expect(label, locale).not.toBe("Tectora");
      expect(label, locale).not.toBe(actorLabel("user"));
    }
    i18n.locale = "cs";
  });
});
