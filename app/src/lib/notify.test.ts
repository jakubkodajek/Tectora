import { afterEach, describe, expect, it, vi } from "vitest";
import { dismiss, hold, notify, toasts } from "./notify.svelte";

afterEach(() => {
  for (const t of [...toasts]) dismiss(t.id);
});

describe("notify", () => {
  it("merges a repeated message into one toast with a count", () => {
    const a = notify({ kind: "error", title: "Could not save", message: "Disk full" });
    const b = notify({ kind: "error", title: "Could not save", message: "Disk full" });
    expect(b).toBe(a);
    expect(toasts).toHaveLength(1);
    expect(toasts[0].count).toBe(2);
  });

  it("keeps two toasts of the same text but different keys apart, each with its own action (audit 2026-11-23 ERR-1403)", () => {
    const first = vi.fn();
    const second = vi.fn();
    const a = notify({ kind: "info", title: "Deleted: Dům", key: "trash-1", action: { label: "Restore", run: first } });
    const b = notify({ kind: "info", title: "Deleted: Dům", key: "trash-2", action: { label: "Restore", run: second } });
    expect(b).not.toBe(a);
    expect(toasts.map((t) => t.count)).toEqual([1, 1]);
    toasts.find((t) => t.id === a)!.action!.run();
    toasts.find((t) => t.id === b)!.action!.run();
    expect(first).toHaveBeenCalledOnce();
    expect(second).toHaveBeenCalledOnce();
    // The same key again is the same toast.
    expect(notify({ kind: "info", title: "Deleted: Dům", key: "trash-2" })).toBe(b);
  });

  it("keeps a sticky notice until it is taken away, also after the pointer left it (audit 2026-11-23 UX-1504)", () => {
    vi.useFakeTimers();
    try {
      const a = notify({ kind: "warning", title: "Another program changed this job", sticky: true });
      const b = notify({ kind: "warning", title: "Ordinary" });
      vi.advanceTimersByTime(60_000);
      expect(toasts.map((t) => t.id)).toEqual([a]);
      hold(a, true);
      hold(a, false);
      vi.advanceTimersByTime(60_000);
      expect(toasts.map((t) => t.id)).toEqual([a]);
      expect(b).not.toBe(a);
      dismiss(a);
      expect(toasts).toHaveLength(0);
    } finally {
      vi.useRealTimers();
    }
  });
});
