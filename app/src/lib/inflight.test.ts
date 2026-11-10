import { describe, expect, it } from "vitest";
import { PendingEdits, exclusive, isBenignError } from "./inflight";

const later = <T>(v: T, ms = 10) => new Promise<T>((r) => setTimeout(() => r(v), ms));

describe("PendingEdits (ERR-004)", () => {
  it("settles only after every tracked edit, also failed ones", async () => {
    const p = new PendingEdits();
    const order: string[] = [];
    p.track(later(1, 20).then(() => order.push("a")));
    p.track(later(0, 5).then(() => Promise.reject(new Error("x")))).catch(() => order.push("b"));
    expect(p.size).toBe(2);
    await p.settled();
    order.push("done");
    expect(order).toEqual(["b", "a", "done"]);
    expect(p.size).toBe(0);
  });

  it("waits for an edit started while it was waiting", async () => {
    const p = new PendingEdits();
    const order: string[] = [];
    p.track(later(0, 5).then(() => {
      p.track(later(0, 20).then(() => order.push("second")));
    }));
    await p.settled();
    order.push("done");
    expect(order).toEqual(["second", "done"]);
  });

  it("resolves at once with nothing in flight", async () => {
    await expect(new PendingEdits().settled()).resolves.toBeUndefined();
  });
});

describe("exclusive (UI-002)", () => {
  it("ignores a second call while the first runs, and reports busy", async () => {
    let runs = 0;
    const states: boolean[] = [];
    const f = exclusive(async () => {
      runs++;
      await later(0, 10);
    }, (b) => states.push(b));
    await Promise.all([f(), f(), f()]);
    expect(runs).toBe(1);
    expect(states).toEqual([true, false]);
    await f();
    expect(runs).toBe(2);
  });

  it("is free again after the action threw", async () => {
    let runs = 0;
    const f = exclusive(async () => {
      runs++;
      throw new Error("boom");
    });
    await expect(f()).rejects.toThrow("boom");
    await expect(f()).rejects.toThrow("boom");
    expect(runs).toBe(2);
  });
});

describe("isBenignError (UI-019)", () => {
  it("knows Chromium's ResizeObserver messages", () => {
    expect(isBenignError("ResizeObserver loop completed with undelivered notifications.")).toBe(true);
    expect(isBenignError(new Error("ResizeObserver loop limit exceeded"))).toBe(true);
    expect(isBenignError({ message: "ResizeObserver loop completed with undelivered notifications." })).toBe(true);
  });

  it("reports everything else", () => {
    expect(isBenignError(new TypeError("Cannot set properties of null (setting 'part')"))).toBe(false);
    expect(isBenignError("Script error.")).toBe(false);
    expect(isBenignError({ code: "io" })).toBe(false);
    expect(isBenignError(null)).toBe(false);
    expect(isBenignError("x ResizeObserver loop completed")).toBe(false);
  });
});
