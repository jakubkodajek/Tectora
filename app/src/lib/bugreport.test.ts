// Bug report helpers (plan target/work/bug-report, criterion 13). Written
// before `bugreport.ts` exists. Assumed API: `buildRequest`, `fitPng`,
// `standalonePlanSvg`, `reportStamp`; the request uses the core's names
// (`description`, `job_id`, `include`, `plan_svg`, `model_png`, `client`, ...).
import { describe, expect, it, vi } from "vitest";
import { DICTS } from "./i18n.svelte";
import { buildRequest, fitPng, leftOutText, reportStamp, standalonePlanSvg } from "./bugreport";
import { i18n, setLocale, t } from "./i18n.svelte";
import { logAction, logNotice, resetReportLog } from "./reportlog";

const everything = { job: true, customer: true, files: true, history: true, snapshot: true, log: true };
const base = {
  description: "The gable is wrong",
  jobId: "novak-dum" as string | null,
  include: everything,
  planSvg: '<svg xmlns="http://www.w3.org/2000/svg"><rect/></svg>' as string | null,
  modelPng: new Uint8Array([0x89, 0x50, 0x4e, 0x47, 1, 2, 3]) as Uint8Array | null,
  editorProject: null as string | null,
  ui: { locale: "cs", theme: "dark", screen: "editor", layout: "split", window: [1440, 900], webgl: true },
};

describe("buildRequest", () => {
  it("carries the ticked flags and the text", () => {
    const r = buildRequest({ ...base, include: { ...everything, customer: false, files: false } });
    expect(r.description).toBe("The gable is wrong");
    expect(r.job_id).toBe("novak-dum");
    expect(r.include).toMatchObject({ job: true, customer: false, files: false, history: true, snapshot: true, log: true });
    expect(r.client.ui).toMatchObject({ locale: "cs", webgl: true });
    expect(Array.isArray(r.client.actions) && Array.isArray(r.client.notices)).toBe(true);
  });

  it("sends the picture as plain numbers (as save_png does) and only when ticked", () => {
    expect(buildRequest(base).model_png).toEqual([0x89, 0x50, 0x4e, 0x47, 1, 2, 3]);
    const r = buildRequest({ ...base, include: { ...everything, snapshot: false } });
    expect(r.model_png ?? null).toBeNull();
    expect(r.plan_svg ?? null).toBeNull();
  });

  it("with no job open the job checkboxes are absent from the request", () => {
    const r = buildRequest({ ...base, jobId: null, planSvg: null });
    expect(r.job_id ?? null).toBeNull();
    for (const k of ["job", "customer", "files", "history"] as const) expect(r.include[k]).not.toBe(true);
    expect(r.include.log).toBe(true);
  });

  it("allows an empty description", () => {
    expect(buildRequest({ ...base, description: "" }).description).toBe("");
  });

  it("cuts a description over 5000 characters instead of sending one the core refuses", () => {
    expect(buildRequest({ ...base, description: "x".repeat(6000) }).description.length).toBeLessThanOrEqual(5000);
  });

  it("stamps the file name in local time, YYYYMMDD-HHMMSS", () => {
    expect(reportStamp(new Date(2026, 10, 28, 14, 30, 12))).toBe("20261128-143012");
    expect(buildRequest({ ...base }).local_stamp).toMatch(/^\d{8}-\d{6}$/);
  });
});

describe("fitPng", () => {
  const cap = 8 * 1024 * 1024;
  it("keeps a picture within the cap as it is", async () => {
    const small = new Uint8Array(1000);
    const rescale = vi.fn();
    expect(await fitPng(small, rescale)).toBe(small);
    expect(rescale).not.toHaveBeenCalled();
  });

  it("downscales a picture over the cap, asking for at most 2000 px on the long side", async () => {
    const big = new Uint8Array(cap + 1);
    const smaller = new Uint8Array(1000);
    const rescale = vi.fn().mockResolvedValue(smaller);
    expect(await fitPng(big, rescale)).toBe(smaller);
    expect(rescale).toHaveBeenCalledOnce();
    expect(rescale.mock.calls[0][0]).toBeLessThanOrEqual(2000);
  });

  it("drops it when it is still too big or cannot be scaled", async () => {
    const big = new Uint8Array(cap + 1);
    expect(await fitPng(big, vi.fn().mockResolvedValue(new Uint8Array(cap + 1)))).toBeNull();
    expect(await fitPng(big, vi.fn().mockResolvedValue(null))).toBeNull();
    expect(await fitPng(big, vi.fn().mockRejectedValue(new Error("no canvas")))).toBeNull();
  });
});

describe("standalonePlanSvg", () => {
  const resolve = (name: string) => ({ "--ink": "#222222", "--brick": "#a8362c" })[name] ?? "";

  it("is a standalone file: starts with <svg>, has xmlns, no theme variable left", () => {
    const out = standalonePlanSvg('<svg viewBox="0 0 10 10"><path stroke="var(--ink)" style="fill:var(--brick, red)"/></svg>', resolve);
    expect(out.startsWith("<svg")).toBe(true);
    expect(out).toContain('xmlns="http://www.w3.org/2000/svg"');
    expect(out).not.toContain("var(--");
    expect(out).toContain("#222222");
    expect(out).toContain("#a8362c");
  });

  it("does not add a second xmlns and falls back to the variable's own fallback", () => {
    const out = standalonePlanSvg('<svg xmlns="http://www.w3.org/2000/svg"><g fill="var(--unknown, #123456)"/></svg>', resolve);
    expect(out.match(/xmlns=/g)).toHaveLength(1);
    expect(out).toContain("#123456");
    expect(out).not.toContain("var(--");
  });
});

describe("texts", () => {
  it("has every report key in all four languages", () => {
    const keys = [
      "report.button", "report.title", "report.what", "report.whatHint", "report.included", "report.inc.job",
      "report.inc.customer", "report.inc.customerHint", "report.inc.files", "report.inc.history", "report.inc.snapshot",
      "report.inc.log", "report.noJob", "report.dest", "report.destNone", "report.send", "report.saveAs", "report.sending",
      "report.done", "report.show", "report.leftOut", "settings.reportsDir", "settings.reportsDirHint",
      "settings.reportsDirUnset", "err.ctx.report",
    ];
    for (const [loc, dict] of Object.entries(DICTS)) {
      expect(keys.filter((k) => !(k in dict)), loc).toEqual([]);
    }
    expect(DICTS.cs["report.button"]).toBe("Nahlásit chybu");
  });
});

// ---- fix round 1 (target/work/bug-report/fix-r1.md, "Test-side API") ----

describe("buildRequest, fix round 1", () => {
  it("sends no steps and notices when the technical record is unticked (ARCH-001, TEST-001)", () => {
    resetReportLog();
    logAction({ kind: "command", name: "place_item", ok: true, ms: 1 });
    logNotice({ kind: "error", code: "io", title: "t", detail: "secret detail" });
    const off = buildRequest({ ...base, include: { ...everything, log: false } });
    expect(off.client.actions).toEqual([]);
    expect(off.client.notices).toEqual([]);
    expect(JSON.stringify(off)).not.toContain("secret detail");
    const on = buildRequest(base);
    expect(on.client.actions).toHaveLength(1);
    expect(on.client.notices).toHaveLength(1);
  });

  it("passes on what could not be pictured, as part and reason (ERR-002)", () => {
    const r = buildRequest({ ...base, planSvg: null, modelPng: null, leftOut: [{ part: "model", reason: "timeout" }, { part: "plan", reason: "too_many_nodes" }] });
    expect(r.client.left_out).toEqual([{ part: "model", reason: "timeout" }, { part: "plan", reason: "too_many_nodes" }]);
    expect(buildRequest(base).client.left_out ?? []).toEqual([]);
  });
});

describe("leftOutText (UX-005, ARCH-004, ERR-003)", () => {
  const parts = ["files", "history", "log", "plan", "model", "preview", "editor_project", "job"];
  const reasons = ["too_large", "too_many", "unreadable", "unavailable", "invalid", "failed", "timeout", "too_many_nodes", "no_webgl"];

  it("has a translated word for every part and reason in all four languages", () => {
    for (const [loc, dict] of Object.entries(DICTS)) {
      expect(parts.map((p) => `report.part.${p}`).filter((k) => !(k in dict)), loc).toEqual([]);
      expect(reasons.map((r) => `report.left.${r}`).filter((k) => !(k in dict)), loc).toEqual([]);
    }
  });

  it("names the part and the reason in the user's language, never the core's English text", () => {
    setLocale("cs");
    const text = leftOutText({ part: "files", reason: "too_many" });
    expect(text).toContain(t("report.part.files"));
    expect(text).toContain(t("report.left.too_many"));
    expect(text).not.toMatch(/job\/files|over the size|too large/i);
    // The label matches the reason: "too many" is not "too large".
    expect(leftOutText({ part: "files", reason: "too_many" })).not.toBe(leftOutText({ part: "files", reason: "too_large" }));
    // An entry from an older core (a plain string) or an unknown code still gives a line.
    expect(leftOutText("job/files (over the size limits)" as never)).toBeTruthy();
    expect(leftOutText({ part: "files", reason: "something_new" })).toContain(t("report.part.files"));
    setLocale(i18n.locale);
  });
});

describe("texts, fix round 1", () => {
  it("has the keys of the reworked dialog in all four languages", () => {
    const keys = ["report.save", "report.savedTo", "report.passOn", "report.destSettings", "report.libraryHint", "report.destMissing"];
    for (const [loc, dict] of Object.entries(DICTS)) expect(keys.filter((k) => !(k in dict)), loc).toEqual([]);
    // UX-001: the main button saves a report; "Send" promises more than it does.
    expect(DICTS.cs["report.save"]).toBe("Uložit hlášení");
    expect(DICTS.en["report.save"]).toMatch(/^Save report/);
  });
});
