// Bug report ring buffer (plan target/work/bug-report, criterion 12). Written
// before `reportlog.ts` exists. Assumed API: `logAction`, `logNotice`,
// `recentActions`, `recentNotices`, `resetReportLog`, `MAX_ACTIONS`,
// `MAX_NOTICES`; `api.ts` feeds it through its invoke wrapper and
// `notify()` feeds the notices.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a), Channel: class {} }));

import { api } from "./api";
import { dismiss, notify, toasts } from "./notify.svelte";
import { MAX_ACTIONS, MAX_NOTICES, logAction, logNotice, recentActions, recentNotices, resetReportLog } from "./reportlog";

beforeEach(() => {
  resetReportLog();
  invoke.mockReset();
});
afterEach(() => {
  for (const t of [...toasts]) dismiss(t.id);
});

describe("report log", () => {
  it("keeps the newest 50 actions and 30 notices, oldest first", () => {
    expect([MAX_ACTIONS, MAX_NOTICES]).toEqual([50, 30]);
    for (let i = 0; i < 80; i++) logAction({ kind: "command", name: `op${i}`, ok: true, ms: 1 });
    for (let i = 0; i < 45; i++) logNotice({ kind: "error", code: "io", title: `t${i}`, detail: "d" });
    const a = recentActions();
    expect(a).toHaveLength(50);
    expect([a[0].name, a[49].name]).toEqual(["op30", "op79"]);
    const n = recentNotices();
    expect(n).toHaveLength(30);
    expect([n[0].title, n[29].title]).toEqual(["t15", "t44"]);
    expect(a.every((x, i) => i === 0 || x.ts_ms >= a[i - 1].ts_ms)).toBe(true);
  });

  it("caps string lengths: 300 characters, a notice's detail 2000", () => {
    logAction({ kind: "command", name: "n".repeat(900), ok: false, code: "c".repeat(900), ms: 1 });
    logNotice({ kind: "error", code: "io", title: "t".repeat(900), detail: "d".repeat(5000) });
    expect(recentActions()[0].name.length).toBeLessThanOrEqual(300);
    expect(String(recentActions()[0].code).length).toBeLessThanOrEqual(300);
    expect(recentNotices()[0].title.length).toBeLessThanOrEqual(300);
    expect(recentNotices()[0].detail!.length).toBeLessThanOrEqual(2000);
  });

  it("returns copies: changing what was read does not change the log", () => {
    logAction({ kind: "edit", name: "a", ok: true, ms: 1 });
    recentActions()[0].name = "changed";
    recentActions().pop();
    expect(recentActions().map((x) => x.name)).toEqual(["a"]);
  });

  it("records a command's name, result and duration, never its arguments", async () => {
    invoke.mockResolvedValueOnce("/jobs");
    invoke.mockRejectedValueOnce({ code: "permission_denied", detail: "secret path /Users/x" });
    await api.projectsDir();
    await expect(api.loadProject("novak-dum")).rejects.toBeTruthy();
    const [ok, failed] = recentActions();
    expect(ok).toMatchObject({ kind: "command", name: "projects_dir", ok: true });
    expect(failed).toMatchObject({ kind: "command", name: "load_project", ok: false, code: "permission_denied" });
    expect(typeof ok.ms).toBe("number");
    expect(ok.ms).toBeGreaterThanOrEqual(0);
    const text = JSON.stringify(recentActions());
    expect(text).not.toContain("novak-dum");
    expect(text).not.toContain("/Users/x");
  });

  it("is fed by notify(): a toast becomes a notice, a repeat does not fill the log", () => {
    notify({ kind: "error", code: "io", title: "Could not save", message: "Disk full", detail: "os error 28" });
    const n = recentNotices();
    expect(n).toHaveLength(1);
    expect(n[0]).toMatchObject({ kind: "error", code: "io", title: "Could not save", detail: "os error 28" });
    notify({ kind: "info", title: "Saved" });
    expect(recentNotices().map((x) => x.kind)).toEqual(["error", "info"]);
  });
});
