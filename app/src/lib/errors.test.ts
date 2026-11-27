import { describe, expect, it } from "vitest";
import { describeBroken, describeCalcError, describeError, describeProblem, describeRefusal, refusalReason, toAppError } from "./errors";
import { DICTS, i18n, t, type Locale } from "./i18n.svelte";

const setLocale = (l: Locale) => (i18n.locale = l);
import { emptyProject } from "./types";

describe("translations", () => {
  it("every locale has every key", () => {
    const cs = Object.keys(DICTS.cs);
    for (const [loc, dict] of Object.entries(DICTS)) {
      const missing = cs.filter((k) => !(k in dict));
      expect(missing, loc).toEqual([]);
    }
  });

  it("never names DACAD in the UI", () => {
    for (const [loc, dict] of Object.entries(DICTS)) {
      const hits = Object.entries(dict).filter(([, v]) => /dacad/i.test(v)).map(([k]) => k);
      expect(hits, loc).toEqual([]);
    }
  });
});

describe("describeError", () => {
  it("translates a backend error code and keeps the detail", () => {
    setLocale("en");
    const d = describeError({ code: "not_found", id: "dum", detail: "dum: project not found" }, "load");
    expect(d.title).toBe("Could not open the job");
    expect(d.message).toContain("no longer in the folder");
    expect(d.detail).toBe("dum: project not found");
  });

  it("revert_out_of_limits says the job is fine, not damaged, in every language (audit 2026-11-23 HUNT-1402)", () => {
    const detail = "dum: the earlier version is outside the current limits and cannot be restored; the job itself is unchanged (value 1000000000 at `parts[0].edges[0].start.x` is out of range)";
    const seen = new Set<string>();
    for (const l of ["cs", "sk", "en", "de"] as Locale[]) {
      setLocale(l);
      const d = describeError({ code: "revert_out_of_limits", id: "dum", detail }, "revert");
      expect(d.code).toBe("revert_out_of_limits");
      expect(d.message, l).toBe(DICTS[l]["err.revert_out_of_limits"]);
      expect(d.message, l).not.toBe(DICTS[l]["err.unreadable"]);
      expect(d.detail).toBe(detail);
      seen.add(d.message);
    }
    expect(seen.size).toBe(4);
    setLocale("en");
    expect(describeError({ code: "revert_out_of_limits", detail }, "revert").message).not.toMatch(/damaged|backup/i);
  });

  it("revert_out_of_limits_damaged does not call a job whose file cannot be read fine, in every language (audit 2026-11-23 ARCH-1503)", () => {
    const detail = "dum: the earlier version is outside the current limits and cannot be restored; the job file itself cannot be read (value 1000000000 at `parts[0].edges[0].start.x` is out of range)";
    const seen = new Set<string>();
    for (const l of ["cs", "sk", "en", "de"] as Locale[]) {
      setLocale(l);
      const d = describeError({ code: "revert_out_of_limits_damaged", id: "dum", detail }, "revert");
      expect(d.code).toBe("revert_out_of_limits_damaged");
      expect(d.message, l).toBe(DICTS[l]["err.revert_out_of_limits_damaged"]);
      expect(d.message, l).not.toBe(DICTS[l]["err.revert_out_of_limits"]);
      expect(d.detail).toBe(detail);
      seen.add(d.message);
    }
    expect(seen.size).toBe(4);
    setLocale("en");
    const m = describeError({ code: "revert_out_of_limits_damaged", detail }, "revert").message;
    expect(m).not.toMatch(/is fine|not changed/i);
    expect(m).toMatch(/cannot be read/);
  });

  it("too_complex says the job is too complex and what to remove, in every language (HUNTIO-004)", () => {
    const detail = "the design is too complex to calculate: its buildings overlap too much";
    const seen = new Set<string>();
    for (const l of ["cs", "sk", "en", "de"] as Locale[]) {
      setLocale(l);
      const d = describeError({ code: "too_complex", detail }, "calc");
      expect(d.code).toBe("too_complex");
      expect(d.message, l).not.toBe(DICTS[l]["err.unknown"]);
      expect(d.message, l).toBe(DICTS[l]["err.too_complex"]);
      expect(d.detail).toBe(detail);
      seen.add(d.message);
    }
    expect(seen.size).toBe(4);
  });

  it("too_many names the pitch break and dormer limits and points to the details (DR-004)", () => {
    for (const l of ["cs", "sk", "en", "de"] as Locale[]) {
      setLocale(l);
      const d = describeError({ code: "too_many", detail: "too many pitch breaks of one wall: 65 (at most 64)" }, "load");
      expect(d.message, l).toContain("64");
      const text = d.message.toLocaleLowerCase(l);
      expect(text, l).toContain(DICTS[l]["tool.dormer"].toLocaleLowerCase(l).slice(0, 4));
      expect(text, l).toContain(DICTS[l]["err.details"].toLocaleLowerCase(l).slice(0, 6));
      expect(d.detail).toBe("too many pitch breaks of one wall: 65 (at most 64)");
    }
  });

  it("too_many for unknown keys names them, not buildings or walls (audit 2026-11-23 ARCH-1201)", () => {
    for (const detail of [
      "deep: the job cannot be opened (unknown keys nested too deep: 40 levels (at most 32))",
      "too much data in unknown keys: 70000 bytes (at most 65536)",
    ]) {
      for (const l of ["cs", "sk", "en", "de"] as Locale[]) {
        setLocale(l);
        const d = describeError({ code: "too_many", detail }, "load");
        expect(d.code).toBe("too_many");
        expect(d.message, l).toBe(DICTS[l]["err.unknown_keys"]);
        expect(d.message, l).toContain("32");
        expect(d.detail).toBe(detail);
      }
    }
    setLocale("en");
    expect(describeError({ code: "too_many", detail: "too many parts: 201 (at most 200)" }, "load").message).toBe(DICTS.en["err.too_many"]);
  });

  it("not_calculated says the result cannot be stored for the offer", () => {
    for (const l of ["cs", "sk", "en", "de"] as Locale[]) {
      setLocale(l);
      const d = describeError({ code: "not_calculated" }, "save");
      expect(d.message, l).toBe(DICTS[l]["err.not_calculated"]);
      expect(d.message, l).not.toBe(DICTS[l]["err.unknown"]);
    }
  });

  it("a file type a job does not carry is refused with its own message (HUNTIO-001)", () => {
    for (const l of ["cs", "sk", "en", "de"] as Locale[]) {
      setLocale(l);
      const d = describeError({ code: "not_attachable", detail: "mail.msg: only drawings, photos and documents can be attached" }, "files");
      expect(d.message, l).toBe(DICTS[l]["err.not_attachable"]);
      expect(d.message, l).not.toBe(DICTS[l]["err.unknown"]);
      expect(d.message, l).toContain("PDF");
      expect(d.detail).toContain("mail.msg");
    }
  });

  it("recognises Tauri's argument errors as an empty field", () => {
    const e = toAppError("invalid args `project` for command `calc`: invalid type: null, expected f64");
    expect(e.code).toBe("invalid_input");
  });

  it("falls back to a generic message for unknown codes and plain errors", () => {
    setLocale("cs");
    expect(describeError({ code: "something_new" }).message).toBe("Nastala neočekávaná chyba.");
    const d = describeError(new Error("boom"));
    expect(d.code).toBe("unknown");
    expect(d.detail).toBe("boom");
  });
});

describe("calculation errors", () => {
  it("explains the calculation error in the user's language only", () => {
    setLocale("de");
    const d = describeCalcError({
      kind: "open_faces",
      code: "E00002",
      message_id: 0xa0f,
      message: "E00002 : Nicht geschlossene Fläche(n) vorhanden",
    });
    expect(d.title).toContain("Flächen sind nicht geschlossen");
    expect(d.code).toBe("open_faces");
    expect(JSON.stringify(d)).not.toContain("DACAD");
  });

  it("names the building whose roof fails on its own", () => {
    // One core error stops every roof of the job; the bar says which building to fix.
    setLocale("cs");
    const p = emptyProject("x");
    p.parts = [
      { name: "Main", edges: [] },
      { name: "Garáž", edges: [] },
    ] as never;
    const e = { kind: "open_faces", message: "x", parts: [1] } as const;
    const d = describeCalcError({ ...e, parts: [1] }, p);
    expect(d.title.startsWith("Budova 2 „Garáž“ – ")).toBe(true);
    expect(d.hint).toContain("Ostatní budovy");
    // One building, or no culprit: as before.
    expect(describeCalcError({ ...e, parts: [] }, p).title).toBe("Střechu nelze spočítat: plochy se neuzavírají");
  });

  it("adds the parallel walls to the kind's own hint, numbered from 1 (TEST-007, ARCH-006)", () => {
    for (const l of ["cs", "sk", "en", "de"] as Locale[]) {
      setLocale(l);
      const plain = describeCalcError({ kind: "cut", message: "x" });
      expect(plain.hint, l).toBe(DICTS[l]["calc.hint.cut"]);
      const d = describeCalcError({ kind: "cut", message: "x", edges: [0, 2] });
      // The kind's hint stays, the walls follow it.
      expect(d.hint.startsWith(DICTS[l]["calc.hint.cut"] + " "), l).toBe(true);
      const walls = d.hint.slice(DICTS[l]["calc.hint.cut"].length);
      expect(walls, l).toMatch(/\b1\b/);
      expect(walls, l).toMatch(/\b3\b/);
      expect(walls, l).not.toMatch(/\b[02]\b/);
      expect(walls, l).not.toContain("{");
      // One wall is no pair: the kind's hint alone.
      expect(describeCalcError({ kind: "cut", message: "x", edges: [4] }).hint, l).toBe(DICTS[l]["calc.hint.cut"]);
      expect(describeCalcError({ kind: "cut", message: "x", edges: [] }).hint, l).toBe(DICTS[l]["calc.hint.cut"]);
    }
    // With several buildings the three sentences add up.
    setLocale("cs");
    const p = emptyProject("x");
    p.parts = [
      { name: "Main", edges: [] },
      { name: "Garáž", edges: [] },
    ] as never;
    const d = describeCalcError({ kind: "open_faces", message: "x", parts: [0], edges: [0, 2] }, p);
    expect(d.hint).toContain(DICTS.cs["calc.hint.open_faces"]);
    expect(d.hint).toContain("Roviny stěn 1 a 3");
    expect(d.hint).toContain("Ostatní budovy");
  });

  it("names the part and wall of an input problem", () => {
    setLocale("cs");
    const p = emptyProject("x");
    p.parts = [
      { name: "Dům", edges: [] },
      { name: "Garáž", edges: [] },
    ] as never;
    expect(describeProblem({ item: "part", index: 1, edge: 2, code: "pitch_range" }, p)).toBe(
      "Budova 2 „Garáž“, stěna 3: sklon musí být od 2° do 178°",
    );
  });
});

describe("refusals of the core (ERR-003)", () => {
  const below = "at a building height of 0.500 m an eave would be -0.500 m, below the ground";

  it("a refused building height is a refusal with the core's reason, in every language", () => {
    for (const l of ["cs", "sk", "en", "de"] as Locale[]) {
      setLocale(l);
      const d = describeRefusal({ code: "refused", detail: below }, "height");
      expect(d, l).not.toBeNull();
      expect(d!.title, l).toBe(DICTS[l]["refuse.title.height"]);
      expect(d!.message, l).toBe(DICTS[l]["refuse.belowGround"]);
      expect(d!.message, l).not.toBe(DICTS[l]["err.invalid_input"]);
      expect(d!.title, l).not.toBe(DICTS[l]["err.action.calc"]);
      expect(d!.detail).toBe(below);
      expect(d!.code).toBe("refused");
    }
  });

  it("names the range and the face the core gives", () => {
    setLocale("cs");
    expect(refusalReason("from_eave 600.000 is out of range (0 to 500 m from the edge of its roof face)")).toBe("Povolený rozsah 0 až 500");
    expect(refusalReason("height 1e9 is out of range (0.5 to 1000 m)")).toBe("Povolený rozsah 0,5 až 1000");
    expect(refusalReason("the object would leave its roof face (from the eave); give a smaller distance")).toBe(DICTS.cs["place.refused"]);
    expect(refusalReason("the object stands on no roof face, so it has no distances to set")).toBe(DICTS.cs["refuse.noFace"]);
    expect(refusalReason("hand-drawn face 3 has fewer than 3 corners")).toBe(DICTS.cs["refuse.other"]);
    expect(describeRefusal({ code: "refused", detail: "x" }, "face")!.title).toBe(DICTS.cs["refuse.title.face"]);
  });

  it("other errors are not refusals; the code itself has a message", () => {
    expect(describeRefusal({ code: "out_of_range", detail: "x" }, "height")).toBeNull();
    expect(describeRefusal(new Error("boom"), "place")).toBeNull();
    setLocale("en");
    expect(describeError({ code: "refused" }).message).toBe(DICTS.en["err.refused"]);
  });
});

describe("settings command errors (ERRU-002)", () => {
  it("keeps a bare settings code as the code, not as an unknown error", () => {
    expect(toAppError("not_writable")).toEqual({ code: "not_writable" });
    expect(toAppError("io")).toEqual({ code: "io" });
    // Any other text stays an unknown error with the text as its detail.
    expect(toAppError("boom").code).toBe("unknown");
  });

  it("says what is wrong with the folder, under the settings title", () => {
    const d = describeError("not_writable", "settings");
    expect(d.code).toBe("not_writable");
    expect(d.title).toBe(t("err.action.settings"));
    expect(d.message).toBe(t("settings.err.not_writable"));
    // From another action the reason is the same.
    expect(describeError("not_writable", "save").message).toBe(t("settings.err.not_writable"));
  });
});

describe("round-3 audit: a job too large to export (ARCH-205)", () => {
  it("names the job's own size, not the attachment limits", () => {
    setLocale("en");
    const job = describeError({ code: "too_large", detail: "job bundle: the job is too large to hand over (17 MB, at most 16 MB); shorten its notes" }, "export");
    expect(job.message).toBe(t("err.job_too_large"));
    expect(job.message).not.toMatch(/256/);
    const file = describeError({ code: "too_large", detail: "x.pdf: larger than 256 MB" }, "files");
    expect(file.message).toBe(t("err.too_large"));
  });
  it("titles a refused stored result as a refusal, not as a failed save (UI-206)", () => {
    setLocale("cs");
    const d = describeError({ code: "no_building" }, "storeResult");
    expect(DICTS.cs["err.action.storeResult"]).toBeTruthy();
    expect(d.title).toBe(DICTS.cs["err.action.storeResult"]);
    expect(d.title).not.toBe(t("err.action.save"));
  });
});

describe("round-5 audit: a result too large to keep (HUNT-405)", () => {
  it("says the roof is too large, not that the stored result is damaged", () => {
    for (const locale of ["cs", "sk", "en", "de"] as const) {
      setLocale(locale);
      const d = describeError(
        { code: "figures_too_large", detail: "the figures are too large to keep: a figure is not a number of a real roof (at most 1000000000)" },
        "storeResult",
      );
      expect(d.message).toBe(DICTS[locale]["err.figures_too_large"]);
      expect(d.message).not.toBe(t("err.unknown"));
    }
  });
});

describe("a job file in another encoding (round 5)", () => {
  it("is called not readable text, not damaged, in every language", () => {
    const was = i18n.locale;
    try {
      for (const loc of ["cs", "sk", "en", "de"] as Locale[]) {
        i18n.locale = loc;
        const d = describeError({ code: "unreadable", detail: "cp1250.tectora: not UTF-8 text (saved in another encoding?), at byte 58" }, "load");
        expect(d.message).toBe(t("err.unreadable_text"));
        expect(d.message).not.toBe(t("err.unreadable"));
        expect(describeError({ code: "unreadable", detail: "expected value at line 1 column 1" }, "load").message).toBe(t("err.unreadable"));
      }
    } finally {
      i18n.locale = was;
    }
    expect(DICTS.en["err.unreadable_text"]).not.toMatch(/damaged/i);
    expect(DICTS.cs["err.unreadable_text"]).not.toMatch(/poškozen/i);
    expect(DICTS.sk["err.unreadable_text"]).not.toMatch(/poškoden/i);
    expect(DICTS.de["err.unreadable_text"]).not.toMatch(/beschädigt/i);
  });
});

describe("round 14: the press's own notices (TEST-1303, UX-1301, UI-1301)", () => {
  const LOCALES = ["cs", "sk", "en", "de"] as Locale[];

  it("busy_edits, busy_open and left_before_action give their own text in every language, as warnings", async () => {
    const { notifyError, toasts, dismiss } = await import("./notify.svelte");
    for (const l of LOCALES) {
      setLocale(l);
      const texts = new Set<string>();
      for (const code of ["busy_edits", "busy_open", "left_before_action"]) {
        const d = describeError({ code, id: "x", detail: "d" }, "revert");
        expect(d.code).toBe(code);
        expect(d.message, `${l} ${code}`).toBe(t(`err.${code}`));
        expect(d.message, `${l} ${code} is not the unknown-code text`).not.toBe(t("err.unknown"));
        texts.add(d.message);
        const id = notifyError({ code, id: "x" }, "revert", "Job");
        expect(toasts.find((x) => x.id === id)?.kind, `${l} ${code}`).toBe("warning");
        dismiss(id);
      }
      expect(texts.size, l).toBe(3);
    }
  });

  it("busy_edits speaks of the press, not of a step, and cs / sk have no comma before nebo / alebo (UX-1301)", () => {
    expect(DICTS.cs["err.busy_edits"]).toContain("před stisknutím");
    expect(DICTS.sk["err.busy_edits"]).toContain("pred stlačením");
    expect(DICTS.en["err.busy_edits"]).toContain("before you pressed it");
    expect(DICTS.de["err.busy_edits"]).toContain("vor dem Klick");
    expect(DICTS.cs["err.busy_edits"]).not.toMatch(/, nebo/);
    expect(DICTS.sk["err.busy_edits"]).not.toMatch(/, alebo/);
    for (const l of LOCALES) expect(DICTS[l]["err.busy_edits"], l).not.toMatch(/tímto krokem|týmto krokom|this step|diesem Schritt/);
  });

  it("Revert all refused is titled in the plural, its own key in every language (UI-1301)", () => {
    for (const l of LOCALES) {
      setLocale(l);
      const all = describeError({ code: "busy", id: "x" }, "revertAll").title;
      expect(all, l).toBe(t("err.action.revertAll"));
      expect(all, l).not.toBe(t("err.action.revert"));
    }
    setLocale("en");
    expect(describeError({ code: "busy" }, "revertAll").title).toBe("Could not revert the changes");
  });
});

describe("round 14: the library's list of files that cannot be read (ARCH-1303)", () => {
  const LOCALES = ["cs", "sk", "en", "de"] as Locale[];

  it("a job refused for too-deep unknown keys is explained as such, never as the operating system's refusal", () => {
    for (const l of LOCALES) {
      setLocale(l);
      const text = describeBroken({ id: "deep", code: "too_many", detail: "unknown keys nested too deep: 33 levels (at most 32)" });
      expect(text, l).toBe(t("err.unknown_keys"));
      expect(text, l).not.toBe(t("err.io"));
    }
  });

  it("each code the store's scan gives has its own text; one errors.ts does not know is an io error", () => {
    setLocale("en");
    for (const code of ["unreadable", "newer_format", "permission_denied", "internal", "disk_full", "foreign_entry", "invalid_id", "not_found", "busy", "too_many", "io"]) {
      expect(describeBroken({ id: "j", code, detail: "" }), code).toBe(t(`err.${code}`, { id: "j" }));
    }
    expect(describeBroken({ id: "j", code: "unreadable", detail: "j: not UTF-8 text" })).toBe(t("err.unreadable_text"));
    expect(describeBroken({ id: "j", code: "something_new", detail: "" })).toBe(t("err.io"));
  });
});
