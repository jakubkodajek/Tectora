import { describe, expect, it } from "vitest";
import { DICTS, i18n } from "./i18n.svelte";
import { foreignSentences, type Foreign } from "./foreign";

const entry = (id: string, kind?: Foreign["kind"], job?: string): Foreign => ({ id, code: "foreign_entry", detail: `/lib/${id}: x`, kind, job });

// Audit 2026-11-22 ARCH-701 / UX-701: one text for every entry said jobs
// "cannot be saved, deleted or copied", also where only one action fails,
// and gave no advice for a file.
describe("foreignSentences", () => {
  // Audit 2026-11-22 ARCH-803: each kind names every operation it blocks
  // (deleting too), and a lock's helper file is not the lock.
  it("says where deleting is refused too, and names a lock's helper file apart from the lock", () => {
    const del: Record<string, RegExp> = { cs: /smazat|mazat/, sk: /vymazať|mazať/, en: /deleted/, de: /löschen/ };
    for (const locale of Object.keys(DICTS) as (keyof typeof DICTS)[]) {
      i18n.locale = locale;
      for (const kind of ["activity", "files", "job_files", "lock"] as const) {
        const [line] = foreignSentences([entry(`.x/${kind}`, kind, "dum")]);
        expect(line, `${locale} ${kind}`).toMatch(del[locale]);
      }
      const [fix] = foreignSentences([entry(".activity/dum.lock.fix", "lock_fix", "dum")], () => "Dům Novák");
      const [lock] = foreignSentences([entry(".activity/dum.lock", "lock", "dum")], () => "Dům Novák");
      expect(fix, locale).toContain("Dům Novák");
      expect(fix, locale).not.toBe(lock);
      expect(fix, locale).not.toMatch(/^lib\./);
    }
    i18n.locale = "cs";
  });

  it("says per kind what is blocked and what to do, in every language", () => {
    const all = [
      entry(".activity", "activity"),
      entry(".files", "files"),
      entry(".trash", "trash"),
      entry("exports", "exports"),
      entry(".files/dum", "job_files", "dum"),
      entry(".activity/dum.lock", "lock", "dum"),
      entry(".activity/.new.lock", "new_lock"),
    ];
    for (const locale of Object.keys(DICTS) as (keyof typeof DICTS)[]) {
      i18n.locale = locale;
      const lines = foreignSentences(all, (id) => (id === "dum" ? "Dům Novák" : id));
      expect(lines, locale).toHaveLength(7);
      expect(new Set(lines).size, locale).toBe(7);
      for (const l of lines) {
        expect(l, locale).not.toContain("{");
        expect(l, locale).not.toMatch(/^lib\./);
      }
      expect(lines[4], locale).toContain("Dům Novák");
      expect(lines[5], locale).toContain("Dům Novák");
    }
    i18n.locale = "cs";
  });

  it("names the trash as the trash and gives advice for a file", () => {
    i18n.locale = "cs";
    const [trash] = foreignSentences([entry(".trash", "trash")]);
    expect(trash).toContain("koš");
    expect(trash).toContain("smazané zakázky");
    expect(trash).toContain("Je-li to soubor");
    expect(trash).not.toContain("ukládat");
    i18n.locale = "en";
    const [exp] = foreignSentences([entry("exports", "exports")]);
    expect(exp).toContain("export");
    expect(exp).not.toMatch(/cannot be saved/);
    expect(exp).toContain("if it is a file");
    const [lock] = foreignSentences([entry(".activity/a.lock", "lock", "a")]);
    expect(lock).toContain("cannot be saved");
    expect(lock).toContain("Move it out");
    i18n.locale = "cs";
  });

  it("says a sentence once, per job once, and has advice for an entry of no known kind", () => {
    i18n.locale = "en";
    const lines = foreignSentences([
      entry(".files/a", "job_files", "a"),
      entry(".files/b", "job_files", "b"),
      entry(".activity/a.lock", "lock", "a"),
      entry(".activity/a.lock.fix", "lock", "a"),
      entry("old"),
    ]);
    expect(lines).toHaveLength(4);
    expect(lines[3]).toContain("Tectora writes nothing through it");
    i18n.locale = "cs";
  });
});
