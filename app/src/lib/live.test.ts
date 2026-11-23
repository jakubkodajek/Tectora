import { describe, expect, it } from "vitest";
import { DICTS, i18n } from "./i18n.svelte";
import { actorLabel, entryKind, entrySummary, entryText, historyText, liveText, receivedRecalculation, refreshedNotice } from "./live.svelte";
import { RECEIVED_ENGINE_ACTOR } from "./types";

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
    // The agent's own words, quoted (audit 2026-11-23 ARCH-1401, design A4).
    expect(liveText(status("changed", "Sklon 42° podle řezu"))).toBe("„Sklon 42° podle řezu“");
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

  // Audit 2026-11-23 ARCH-1401 (design A3): these two tests passed bare
  // strings, which the app no longer translates by their words. They now
  // pass entries: a revert by its `undid_from` (or the user's revert of
  // Tectora 0.8.0, told by its sentence), and the entry a revert quotes is
  // translated only when the list has it with a kind of its own.
  it("translates the revert entries the core writes, nested too", () => {
    i18n.locale = "cs";
    const log = [
      { ts_ms: 2, actor: "user", summary: "Reverted: X", undid_from: 1 },
      { ts_ms: 1, actor: "claude", summary: "X" },
    ];
    expect(entryText({ actor: "user", summary: "Reverted: Sklon 45°", undid_from: 1 })).toBe("Vráceno: Sklon 45°");
    expect(entryText({ actor: "user", summary: "Reverted 2 changes since: Sklon 45°", undid_from: 1 })).toBe("Vráceno změn: 2, od: Sklon 45°");
    expect(entryText({ actor: "user", summary: "Reverted: Reverted: X", undid_from: 2 }, log)).toBe("Vráceno: Vráceno: X");
    // The quoted entry not in the list: its words as they are.
    expect(entryText({ actor: "user", summary: "Reverted: Reverted: X", undid_from: 7 }, log)).toBe("Vráceno: Reverted: X");
    expect(entryText({ actor: "user", summary: "Sklon 45°" })).toBe("Sklon 45°");
    // The user's revert of Tectora 0.8.0 (no `undid_from`) is told by its sentence.
    expect(entryText({ actor: "user", summary: "Reverted: Sklon 45°" })).toBe("Vráceno: Sklon 45°");
    expect([entryKind({ actor: "user", summary: "Reverted 3 changes since: x" }), entryKind({ actor: "user", summary: "Reverted: x" }), entryKind({ actor: "user", summary: "Revert the eave" })]).toEqual(["revert", "revert", null]);
    i18n.locale = "de";
    expect(entryText({ actor: "user", summary: "Reverted: Pitch", undid_from: 1 })).toBe("Verworfen: Pitch");
  });

  it("translates a revert with the AI's reason, in every language (UX-002)", () => {
    const want: Record<string, string> = {
      cs: "Vráceno: na část (důvod: zpět)",
      sk: "Vrátené: na část (dôvod: zpět)",
      en: "Reverted: na část (reason: zpět)",
      de: "Verworfen: na část (Grund: zpět)",
    };
    const revert = (summary: string, undid_from = 1) => ({ actor: "claude", summary, undid_from });
    for (const [loc, text] of Object.entries(want)) {
      i18n.locale = loc as typeof i18n.locale;
      expect(entryText(revert("Reverted: na část (reason: zpět)"))).toBe(text);
    }
    i18n.locale = "cs";
    expect(entryText(revert("Reverted 2 changes since: X (reason: back)"))).toBe("Vráceno změn: 2, od: X (důvod: back)");
    // A revert of a revert with reasons: the outer reason is the last one.
    const log = [{ ts_ms: 2, actor: "claude", summary: "Reverted: X (reason: a)", undid_from: 1 }];
    expect(entryText(revert("Reverted: Reverted: X (reason: a) (reason: b)", 2), log)).toBe("Vráceno: Vráceno: X (důvod: a) (důvod: b)");
    // Entries written before the fixed form: `reason (Reverted: …)`.
    expect(entryText(revert("undo (Reverted: to part)"))).toBe("Vráceno: to part (důvod: undo)");
    expect(entryKind(revert("undo (Reverted: to part)"))).toBe("revert");
    // An AI summary that only mentions a reason is left as it is (its own words, quoted).
    expect(entryText({ actor: "claude", summary: "Pitch 45 (reason: drawing)" })).toBe("„Pitch 45 (reason: drawing)“");
  });

  // Audit 2026-11-23 ARCH-1401: an AI's summary that reads like Tectora's
  // own sentence (a revert, a recalculation, a put-back) is the AI's
  // words: shown as written, in quotation marks, never translated.
  it("shows an AI's summary that reads like Tectora's sentence as the AI's quoted words, in every language", () => {
    const spoofs = [
      "Reverted: Customer",
      "Reverted 2 changes since: Notes",
      "Stored result recalculated (Tectora 0.9.0 -> 0.9.1): roof area 152.73 -> 152.73 m2",
      "Offer result restored (Tectora 0.9.1 -> 0.9.0)",
      "Offer result restored (Tectora 0.9.1 -> 0.9.0) (reason: signed)",
    ];
    const quote: Record<string, [string, string]> = { cs: ["„", "“"], sk: ["„", "“"], en: ["“", "”"], de: ["„", "“"] };
    for (const [loc, [open, close]] of Object.entries(quote)) {
      i18n.locale = loc as typeof i18n.locale;
      for (const summary of spoofs) {
        const a = { ts_ms: 9, actor: "claude", summary };
        expect(entryKind(a), summary).toBeNull();
        expect(entryText(a), `${loc} ${summary}`).toBe(`${open}${summary}${close}`);
        expect(historyText(a, [], 9), `${loc} ${summary}`).toBe(`${open}${summary}${close}`);
        // With the reason field the AI cannot set either.
        expect(entryText({ ...a, reason: "signed" }), `${loc} ${summary}`).toBe(`${open}${summary}${close}`);
      }
      // The same words with the core's kind read as Tectora's sentence.
      expect(entryText({ actor: "claude", summary: spoofs[0], kind: "revert" })).not.toContain(open);
      expect(entryText({ actor: "tectora", summary: spoofs[2], kind: "refresh" })).toContain("0.9.0 → 0.9.1");
      expect(entryText({ actor: "user", summary: spoofs[3], kind: "restore" })).toContain("0.9.1 → 0.9.0");
      // A revert by the AI of its own disguised entry translates only its own sentence.
      const log = [{ ts_ms: 9, actor: "claude", summary: spoofs[2] }];
      const real = entryText({ actor: "claude", summary: `Reverted: ${spoofs[2]}`, undid_from: 9, kind: "revert" }, log);
      expect(real, loc).toBe(DICTS[loc as keyof typeof DICTS]["chg.revertedEntry"].replace("{s}", spoofs[2]));
    }
    i18n.locale = "cs";
  });

  // ARCH-1401 guard: no component translates a summary by its words.
  it("is called by no component: they go by the entry's kind", () => {
    const files = import.meta.glob("./**/*.svelte", { eager: true, query: "?raw", import: "default" }) as Record<string, string>;
    expect(Object.keys(files).length).toBeGreaterThan(10);
    for (const [path, text] of Object.entries(files)) expect(text, path).not.toMatch(/\bentrySummary\(/);
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
      // ARCH-1401: an AI's revert is one by its `undid_from`, not by its words.
      expect(historyText({ actor: "claude", summary: reverted, reason: "Mistake (reason: typo) fixed", undid_from: 1 }, [])).toBe(v);
    }
    i18n.locale = "cs";
  });
});

// Audit 2026-11-23 HUNT-1002, ERR-1003: entries written before the core
// kept the reason whole had the whole summary capped at 500 characters, so
// it ends inside the reason or inside the sentence before it ("…"). The
// reason field holds it whole: the line shows the translated head and it.
describe("an entry whose summary was cut by the cap (HUNT-1002, ERR-1003)", () => {
  const cap = (s: string) => (s.length <= 500 ? s : `${s.slice(0, 499)}…`);
  const reason = `The customer signed these figures. ${"y".repeat(415)}`;
  const restored = cap(`Offer result restored (Tectora 0.9.0 -> 0.8.0): roof area 152.73 -> 154.73 m2 (reason: ${reason})`);
  const what = `Note. ${"x".repeat(480)}`;
  const reverted = cap(`Reverted: ${what} (reason: Customer asked)`);
  const head: Record<string, [string, string, string]> = {
    cs: ["Původní čísla nabídky vrácena (Tectora 0.9.0 → 0.8.0)", "Vráceno: Note. ", "důvod"],
    sk: ["Pôvodné čísla ponuky vrátené (Tectora 0.9.0 → 0.8.0)", "Vrátené: Note. ", "dôvod"],
    en: ["Offer figures put back (Tectora 0.9.0 → 0.8.0)", "Reverted: Note. ", "reason"],
    de: ["Ursprüngliche Zahlen des Angebots zurückgeholt (Tectora 0.9.0 → 0.8.0)", "Verworfen: Note. ", "Grund"],
  };
  it("is cut as the store cut it", () => {
    expect(restored).toHaveLength(500);
    expect(restored.endsWith(`${reason})`)).toBe(false);
    expect(reverted).toHaveLength(500);
    expect(reverted).not.toContain(" (reason: ");
  });
  it("shows the translated sentence and the whole reason, in every language", () => {
    for (const [loc, [put, rev, word]] of Object.entries(head)) {
      i18n.locale = loc as typeof i18n.locale;
      expect(entrySummary(restored, reason), loc).toBe(`${put} (${word}: ${reason})`);
      // ARCH-1401: a put-back is one by its `restores`, not by its words.
      expect(historyText({ actor: "claude", summary: restored, reason, restores: 1 }, []), loc).toBe(`${put} (${word}: ${reason})`);
      const r = entrySummary(reverted, "Customer asked");
      expect(r.startsWith(rev), `${loc}: ${r.slice(0, 40)}`).toBe(true);
      expect(r.endsWith(`… (${word}: Customer asked)`), `${loc}: ${r.slice(-40)}`).toBe(true);
      expect(r, loc).not.toContain(" (r…");
    }
    i18n.locale = "cs";
  });
  it("finds the cut reason after a reason-like text in the AI's own summary", () => {
    i18n.locale = "en";
    const long = `Mistake fixed ${"z".repeat(480)}`;
    const s = cap(`Reverted: Note (reason: x) (reason: ${long})`);
    expect(entrySummary(s, long)).toBe(`Reverted: Note (reason: x) (reason: ${long})`);
    // The cut fell before the reason began: the AI's "(reason: x)" is not taken for it.
    const s2 = cap(`Reverted: Note (reason: x) ${"w".repeat(480)} (reason: Customer asked)`);
    expect(entrySummary(s2, "Customer asked")).toMatch(/^Reverted: Note \(reason: x\) w+… \(reason: Customer asked\)$/);
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
    // The AI's own words, quoted (ARCH-1401, design A4).
    expect(historyText({ actor: "claude", summary: "Pitch 45" }, log)).toBe("“Pitch 45”");
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

// Audit 2026-11-23 UX-1003 (owner decision: bundles carry no offer
// figures): a job received after the sender's Tectora recalculated its
// stored result says so on its card, from the received entry, without
// the put-back (the offer's own figures stayed with the sender).
describe("receivedRecalculation (UX-1003)", () => {
  const entry = (ts_ms: number, actor: string, summary: string, fields: string[] = []) => ({
    ts_ms,
    actor,
    summary,
    changes: fields.map((field) => ({ kind: "modified" as const, field, before: null, after: null })),
  });
  const RECALC = "Stored result recalculated (Tectora 0.8.0 -> 0.9.0): roof area 150.00 -> 152.73 m2";
  const job = (engine_version = "0.9.0") => ({ result_snapshot: { engine_version } });
  it("reads the received engine entry the stored result still is", () => {
    const log = [entry(30, "claude", "Notes", ["notes"]), entry(20, RECEIVED_ENGINE_ACTOR, RECALC, ["result_snapshot"]), entry(10, "user", "Status", ["result_snapshot", "status"])];
    expect(receivedRecalculation(log, job())).toEqual({ at: 20, from: "0.8.0", to: "0.9.0", area: 150 });
    const lines = entry(20, RECEIVED_ENGINE_ACTOR, "Stored result recalculated (Tectora 0.8.0 -> 0.9.0): line lengths G, K", ["result_snapshot"]);
    expect(receivedRecalculation([lines], job())).toEqual({ at: 20, from: "0.8.0", to: "0.9.0", area: null });
  });
  it("is null once a later entry stored another result, for this engine's own entry and for another engine", () => {
    expect(receivedRecalculation([entry(30, "user", "Stored", ["result_snapshot"]), entry(20, RECEIVED_ENGINE_ACTOR, RECALC, ["result_snapshot"])], job())).toBeNull();
    expect(receivedRecalculation([entry(20, "tectora", RECALC, ["result_snapshot"])], job())).toBeNull();
    expect(receivedRecalculation([entry(20, RECEIVED_ENGINE_ACTOR, RECALC, ["result_snapshot"])], job("0.8.0"))).toBeNull();
    expect(receivedRecalculation([entry(20, RECEIVED_ENGINE_ACTOR, RECALC, ["result_snapshot"])], { result_snapshot: null })).toBeNull();
    expect(receivedRecalculation([entry(20, RECEIVED_ENGINE_ACTOR, "Something else", ["result_snapshot"])], job())).toBeNull();
    expect(receivedRecalculation([], job())).toBeNull();
  });
});
