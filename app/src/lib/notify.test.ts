import { afterEach, describe, expect, it, vi } from "vitest";
import { dismiss, notify, toasts } from "./notify.svelte";

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
});
