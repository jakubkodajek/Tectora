// Bug reports are sent through the relay (plan-delivery D12, D13, D17; ADR
// 0008). Written before the helpers exist. Assumed API, see "Test-side API" in
// target/work/bug-report/plan-delivery.md:
//   bugreport.ts:     reportToast, reportFailure, dialogLayout, cleanReporterName,
//                     loadReporterName, saveReporterName, MAX_REPORTER_NAME,
//                     outboxRowText, buildRequest({ reporterName })
//   report.svelte.ts: report.outbox, setOutboxCounts, outboxIndicator
import { afterEach, describe, expect, it } from "vitest";
import { DICTS, i18n, setLocale, t } from "./i18n.svelte";
import {
  MAX_REPORTER_NAME,
  buildRequest,
  cleanReporterName,
  dialogLayout,
  loadReporterName,
  outboxRowText,
  reportFailure,
  reportToast,
  saveReporterName,
  sendNowToast,
} from "./bugreport";
import { describeError } from "./errors";
import { outboxIndicator, report, setOutboxCounts } from "./report.svelte";

const original = i18n.locale;
afterEach(() => setLocale(original));

const everything = { job: true, customer: true, files: true, history: true, snapshot: true, log: true };
const base = {
  description: "d",
  jobId: "novak-dum" as string | null,
  include: everything,
  planSvg: null,
  modelPng: null,
  editorProject: null,
  ui: { locale: "cs", theme: "dark", screen: "editor", layout: "split", window: [1440, 900], webgl: true },
};

const sent = { state: "sent" as const, file_name: "tectora-report-20261128-143012-app.zip", size: 1234, left_out: [] as { part: string; reason: string }[] };

function fakeStorage(initial: Record<string, string> = {}) {
  const m = new Map(Object.entries(initial));
  return {
    getItem: (k: string) => m.get(k) ?? null,
    setItem: (k: string, v: string) => void m.set(k, v),
    removeItem: (k: string) => void m.delete(k),
    dump: () => Object.fromEntries(m),
  };
}

describe("the toast of a finished report (D12)", () => {
  it("says sent, closes the dialog, clears the draft, and shows no issue link", () => {
    setLocale("en");
    const plan = reportToast({ ...sent, issue_url: "https://github.com/o/r/issues/7" });
    expect(plan.kind).toBe("info");
    expect(plan.title).toBe(t("report.sent"));
    expect(plan.title).toBe("Report sent. Thank you.");
    expect(plan.closeDialog).toBe(true);
    expect(plan.clearDraft).toBe(true);
    const all = JSON.stringify(plan);
    expect(all).not.toContain("github.com");
    expect(all).not.toMatch(/google|issue/i);
  });

  it("adds what was left out to a sent report", () => {
    setLocale("en");
    const plan = reportToast({ ...sent, left_out: [{ part: "files", reason: "too_large" }] });
    expect(plan.message).toContain(t("report.leftOut", { what: `${t("report.part.files")}: ${t("report.left.too_large")}` }));
  });

  it("says a waiting report is safe and will be sent later", () => {
    setLocale("en");
    const plan = reportToast({ ...sent, state: "waiting" });
    expect(plan.kind).toBe("info");
    expect(`${plan.title}\n${plan.message}`).toContain(t("report.queued"));
    expect(plan.closeDialog).toBe(true);
    expect(plan.clearDraft).toBe(true);
  });

  it("keeps the Save as result as it was: file name, pass it on, show in folder", () => {
    setLocale("en");
    const plan = reportToast({ ...sent, state: "saved", path: "/home/u/report.zip" });
    expect(plan.title).toBe(t("report.done"));
    expect(plan.message).toContain(sent.file_name);
    expect(plan.message).toContain(t("report.passOn"));
    expect(plan.closeDialog).toBe(true);
    expect(plan.clearDraft).toBe(true);
    expect(plan.showPath).toBe("/home/u/report.zip");
  });

  it("is the same in every language: a text, never a key", () => {
    for (const loc of ["cs", "sk", "en", "de"] as const) {
      setLocale(loc);
      for (const state of ["sent", "waiting", "saved"] as const) {
        const p = reportToast({ ...sent, state, path: "/x.zip" });
        expect(`${p.title}${p.message}`, `${loc} ${state}`).not.toMatch(/report\.\w+/);
      }
    }
  });
});

describe("a report that could not be queued (D12)", () => {
  it("keeps the dialog and the text, and offers Save as, when it is too large", () => {
    setLocale("en");
    const f = reportFailure("too_large");
    expect(f.closeDialog).toBe(false);
    expect(f.clearDraft).toBe(false);
    expect(f.hint).toBe(t("report.tooLargeHint"));
  });

  it("explains a full outbox with its own text and also keeps the draft", () => {
    setLocale("en");
    const f = reportFailure("outbox_full");
    expect(f.closeDialog).toBe(false);
    expect(f.clearDraft).toBe(false);
    expect(f.hint).toBe(t("report.err.outbox_full"));
  });

  it("gives no hint for other errors (the usual error toast shows them)", () => {
    expect(reportFailure("io").hint).toBeNull();
    expect(reportFailure("io").closeDialog).toBe(false);
    expect(reportFailure("io").clearDraft).toBe(false);
  });
});

describe("the dialog layout (D12)", () => {
  it("with a relay: the info line, Send primary, Save as secondary", () => {
    const l = dialogLayout(true);
    expect(l.infoKey).toBe("report.sendInfo");
    expect(l.primary).toBe("send");
    expect(l.secondary).toBe("saveAs");
  });

  it("without a relay: the off line and Save as is the only action", () => {
    const l = dialogLayout(false);
    expect(l.infoKey).toBe("report.sendOff");
    expect(l.primary).toBe("saveAs");
    expect(l.secondary).toBeNull();
  });

  it("the info line names no folder, in any language", () => {
    for (const loc of ["cs", "sk", "en", "de"] as const) {
      for (const k of ["report.sendInfo", "report.sendOff"]) {
        expect(DICTS[loc][k], `${loc} ${k}`).toBeTruthy();
        expect(DICTS[loc][k]).not.toMatch(/folder|složk|priečinok|ordner/i);
      }
    }
    expect(DICTS.cs["report.send"]).toBe("Odeslat");
  });
});

describe("the optional name field (Q4)", () => {
  it("is cleaned and cut at 80 characters", () => {
    expect(MAX_REPORTER_NAME).toBe(80);
    expect(cleanReporterName("  Petr Novák  ")).toBe("Petr Novák");
    expect(cleanReporterName("a\u0000b\u0007c\nd‮E")).toBe("abcdE");
    expect(cleanReporterName("x".repeat(200)).length).toBe(80);
    expect(cleanReporterName("   ")).toBe("");
  });

  it("is remembered on this computer only", () => {
    const s = fakeStorage();
    expect(loadReporterName(s)).toBe("");
    saveReporterName(s, "  Petr  ");
    expect(loadReporterName(s)).toBe("Petr");
    expect(Object.keys(s.dump())).toHaveLength(1);
    saveReporterName(s, "");
    expect(loadReporterName(s)).toBe("");
    expect(Object.keys(s.dump())).toHaveLength(0);
  });

  it("survives a storage that throws", () => {
    const broken = {
      getItem: () => {
        throw new Error("denied");
      },
      setItem: () => {
        throw new Error("denied");
      },
      removeItem: () => {
        throw new Error("denied");
      },
    };
    expect(loadReporterName(broken)).toBe("");
    expect(() => saveReporterName(broken, "Petr")).not.toThrow();
  });

  it("goes into the request only when filled", () => {
    expect(buildRequest({ ...base, reporterName: "Petr" }).client.user_name).toBe("Petr");
    expect(buildRequest({ ...base, reporterName: "  " }).client.user_name ?? null).toBeNull();
    expect(buildRequest({ ...base }).client.user_name ?? null).toBeNull();
    expect(buildRequest({ ...base, reporterName: "x".repeat(300) }).client.user_name?.length).toBe(80);
  });
});

describe("the outbox indicator (D13)", () => {
  it("is hidden at zero", () => {
    setOutboxCounts({ waiting: 0, failed: 0, local: 0 });
    expect(outboxIndicator().visible).toBe(false);
    expect(outboxIndicator().count).toBe(0);
  });

  // ARCH-002 / UI-T2 (fix round 2): a report only kept here is reachable
  // from the count; it is not hidden behind a number that stays zero.
  it("counts reports that are only kept here, so they can be listed, saved and discarded", () => {
    setOutboxCounts({ waiting: 0, failed: 0, local: 4 });
    expect(outboxIndicator().visible).toBe(true);
    expect(outboxIndicator().count).toBe(4);
  });

  it("shows every queued report and says how many could not be sent", () => {
    setLocale("en");
    setOutboxCounts({ waiting: 2, failed: 1, local: 5 });
    const i = outboxIndicator();
    expect(i.visible).toBe(true);
    expect(i.count).toBe(8);
    expect(i.failed).toBe(1);
    expect(i.label).toBe(t("report.outbox.countFailed", { count: 8, failed: 1 }));
  });

  it("keeps the plain label while nothing failed", () => {
    setLocale("en");
    setOutboxCounts({ waiting: 3, failed: 0, local: 0 });
    const i = outboxIndicator();
    expect(i.failed).toBe(0);
    expect(i.label).toBe(t("report.outbox.count", { count: 3 }));
    expect(i.label).toBe("Reports waiting to be sent: 3");
  });

  it("drops when the outbox event says it is empty", () => {
    setOutboxCounts({ waiting: 1, failed: 0, local: 0 });
    expect(outboxIndicator().visible).toBe(true);
    setOutboxCounts({ waiting: 0, failed: 0, local: 0 });
    expect(outboxIndicator().visible).toBe(false);
    expect(report.outbox).toEqual({ waiting: 0, failed: 0, local: 0 });
  });

  it("takes only counts from the event, nothing else (an untrusted payload)", () => {
    setOutboxCounts({ waiting: -4, failed: Number.NaN, local: "x" } as never);
    expect(outboxIndicator().visible).toBe(false);
    setOutboxCounts(null as never);
    expect(outboxIndicator().count).toBe(0);
    setOutboxCounts({ waiting: 1e12, failed: 0, local: 0 });
    expect(Number.isFinite(outboxIndicator().count)).toBe(true);
  });
});

describe("the rows of the outbox list (D13)", () => {
  const row = { file_name: "tectora-report-20261128-143012-app.zip", state: "waiting", created_ms: 1_700_000_000_000, job_id: null as string | null, last_error: null as string | null };

  it("shows 'no job' for a report without one and the state in words", () => {
    setLocale("en");
    const r = outboxRowText(row);
    expect(r.job).toBe(t("report.outbox.noJob"));
    expect(r.state).toBe("waiting");
    expect(r.error).toBeNull();
    expect(outboxRowText({ ...row, state: "failed" }).state).toBe("cannot be sent");
    expect(outboxRowText({ ...row, state: "local" }).state).toBe("saved here only");
  });

  it("shows the job name when there is one", () => {
    expect(outboxRowText({ ...row, job_name: "Novák dům" } as never).job).toBe("Novák dům");
  });

  it("explains a known error code and shows an unknown one as it is, in monospace by the UI", () => {
    setLocale("en");
    expect(outboxRowText({ ...row, last_error: "offline" }).error).toEqual({ code: "offline", text: "no connection" });
    expect(outboxRowText({ ...row, last_error: "bad_key" }).error).toEqual({ code: "bad_key", text: "this version can no longer send reports; update the app" });
    expect(outboxRowText({ ...row, last_error: "weird_code" }).error).toEqual({ code: "weird_code", text: null });
  });
});

describe("texts (D17)", () => {
  const added = [
    "report.sendInfo", "report.sendOff", "report.sent", "report.queued", "report.tooLargeHint",
    "report.outbox.count", "report.outbox.title", "report.outbox.sendNow", "report.outbox.saveAs",
    "report.outbox.discard", "report.outbox.discardAsk", "report.outbox.noJob",
    "report.state.waiting", "report.state.failed", "report.state.local",
    "report.err.offline", "report.err.rate_limited", "report.err.bad_key", "report.err.server",
    "report.err.outbox_full", "err.ctx.reportSave", "report.yourName",
  ];
  const removed = [
    "report.dest", "report.destNone", "report.destSettings", "report.destMissing", "report.savedTo", "report.save",
    "settings.reportsDir", "settings.reportsDirHint", "settings.reportsDirUnset", "folderReq.reportsDir",
  ];

  it("has every new key in all four languages", () => {
    for (const [loc, dict] of Object.entries(DICTS)) expect(added.filter((k) => !dict[k]), loc).toEqual([]);
  });

  it("has no removed key left in any language", () => {
    for (const [loc, dict] of Object.entries(DICTS)) expect(removed.filter((k) => k in dict), loc).toEqual([]);
  });

  it("keeps the placeholders of the count text", () => {
    for (const [loc, dict] of Object.entries(DICTS)) expect(dict["report.outbox.count"], loc).toContain("{count}");
  });

  it("says what the plan says in Czech and English", () => {
    expect(DICTS.en["report.sendInfo"]).toBe("The report is sent to the developers of Tectora.");
    expect(DICTS.cs["report.sendInfo"]).toBe("Hlášení se odešle vývojářům aplikace Tectora.");
    expect(DICTS.cs["report.sent"]).toBe("Hlášení odesláno. Děkujeme.");
    expect(DICTS.en["report.sending"]).toBe("Sending the report…");
    expect(DICTS.cs["report.sending"]).toBe("Odesílám hlášení…");
    expect(DICTS.en["err.ctx.report"]).toBe("The report could not be sent");
    expect(DICTS.cs["err.ctx.report"]).toBe("Hlášení se nepodařilo odeslat");
    expect(DICTS.en["err.ctx.reportSave"]).toBe("The report could not be saved");
    expect(DICTS.cs["err.ctx.reportSave"]).toBe("Hlášení se nepodařilo uložit");
  });

  it("names neither Google, GitHub nor the relay in any report text", () => {
    for (const [loc, dict] of Object.entries(DICTS)) {
      for (const k of Object.keys(dict).filter((k) => k.startsWith("report."))) {
        expect(dict[k], `${loc} ${k}`).not.toMatch(/google|github|apps script|relay|drive|script\.google/i);
      }
    }
  });

  it("has a text for every code the relay and the outbox can give", () => {
    for (const code of ["offline", "rate_limited", "bad_key", "server", "outbox_full"]) {
      for (const [loc, dict] of Object.entries(DICTS)) expect(dict[`report.err.${code}`], `${loc} ${code}`).toBeTruthy();
    }
  });
});

// ---- fix round 2 (target/work/bug-report/fix-r2.md) ----

describe("a report the relay refused for good (ERR-001, UI-003)", () => {
  const failed = { ...sent, state: "failed" as const, last_error: "bad_key" };

  it("is announced as not sent, with the reason, never as waiting or sent", () => {
    setLocale("en");
    const plan = reportToast(failed);
    expect(plan.kind).toBe("error");
    const all = `${plan.title}\n${plan.message}`;
    expect(all).not.toContain(t("report.queued"));
    expect(all).not.toContain(t("report.sent"));
    expect(plan.title).toBe(t("report.failed"));
    expect(all).toContain(t("report.err.bad_key"));
    expect(all).toContain("bad_key");
    // The report is safe in the list: the dialog closes and the text is cleared.
    expect(plan.closeDialog).toBe(true);
    expect(plan.clearDraft).toBe(true);
  });

  it("names Save as as the way on", () => {
    setLocale("en");
    const plan = reportToast(failed);
    expect(`${plan.title}\n${plan.message}`).toContain(t("report.saveAs"));
  });

  it("is in every language a text, never a key", () => {
    for (const loc of ["cs", "sk", "en", "de"] as const) {
      setLocale(loc);
      const p = reportToast(failed);
      expect(`${p.title}${p.message}`, loc).not.toMatch(/report\.[\w.]+/);
    }
  });
});

describe("the toast of Send now (ERR-010, UI-002)", () => {
  it("says how many were sent", () => {
    setLocale("en");
    const p = sendNowToast({ sent: 2, waiting: 0, failed: 0 });
    expect(p.kind).toBe("info");
    expect(`${p.title}\n${p.message}`).toContain(t("report.outbox.sentNow", { count: 2 }));
  });

  it("says nothing could be sent and why, when nothing went", () => {
    setLocale("en");
    const p = sendNowToast({ sent: 0, waiting: 1, failed: 0, last_error: "offline" });
    expect(p.kind).toBe("error");
    expect(`${p.title}\n${p.message}`).toContain(t("report.outbox.sendNowFailed", { code: "offline" }));
    const f = sendNowToast({ sent: 0, waiting: 0, failed: 1, last_error: "bad_key" });
    expect(f.kind).toBe("error");
    expect(`${f.title}\n${f.message}`).toContain("bad_key");
  });

  it("says both when some went and some did not", () => {
    setLocale("en");
    const p = sendNowToast({ sent: 1, waiting: 1, failed: 0, last_error: "server" });
    const all = `${p.title}\n${p.message}`;
    expect(all).toContain(t("report.outbox.sentNow", { count: 1 }));
    expect(all).toContain("server");
  });
});

describe("texts for every code (ERR-009, ERR-011)", () => {
  // Every code the relay client, the queue and the commands can give.
  const codes = ["offline", "rate_limited", "bad_key", "server", "outbox_full", "relay_auth", "bad_answer", "bad_request", "too_large", "io", "unreadable", "busy"];

  it("has a text for each in all four languages", () => {
    for (const [loc, dict] of Object.entries(DICTS)) {
      expect(codes.filter((c) => !dict[`report.err.${c}`]), loc).toEqual([]);
    }
  });

  it("shows that text in an outbox row, not a bare code", () => {
    setLocale("en");
    for (const c of codes) {
      const r = outboxRowText({ file_name: "x", state: "failed", created_ms: 1, job_id: null, last_error: c });
      expect(r.error?.text, c).toBeTruthy();
    }
  });

  it("says the report limit is 20 MB, not the job files' 256 MB", () => {
    for (const [loc, dict] of Object.entries(DICTS)) {
      expect(dict["report.err.too_large"], loc).toMatch(/20/);
      expect(dict["report.err.too_large"], loc).not.toMatch(/256/);
    }
  });

  it("has its own titles for saving and discarding", () => {
    for (const [loc, dict] of Object.entries(DICTS)) {
      expect(dict["err.ctx.reportDiscard"], loc).toBeTruthy();
      expect(dict["err.ctx.reportDiscard"], loc).not.toBe(dict["err.ctx.report"]);
    }
  });

  it("explains a full outbox with the report text, not 'unexpected error'", () => {
    setLocale("en");
    const d = describeError({ code: "outbox_full" }, "report");
    expect(d.message).not.toBe(t("err.unknown"));
    expect(d.message).toContain(t("report.err.outbox_full"));
  });

  it("does not call a failed discard 'could not be sent'", () => {
    setLocale("en");
    const d = describeError({ code: "busy" }, "reportDiscard" as never);
    expect(d.title).toBe(t("err.ctx.reportDiscard"));
    expect(d.message).not.toMatch(/saving the job/i);
  });

  it("has the new texts in all four languages", () => {
    const keys = ["report.failed", "report.outbox.countFailed", "report.outbox.sentNow", "report.outbox.sendNowFailed"];
    for (const [loc, dict] of Object.entries(DICTS)) expect(keys.filter((k) => !dict[k]), loc).toEqual([]);
    for (const [loc, dict] of Object.entries(DICTS)) {
      expect(dict["report.outbox.countFailed"], loc).toContain("{count}");
      expect(dict["report.outbox.countFailed"], loc).toContain("{failed}");
      expect(dict["report.outbox.sendNowFailed"], loc).toContain("{code}");
      expect(dict["report.outbox.sentNow"], loc).toContain("{count}");
    }
  });
});
