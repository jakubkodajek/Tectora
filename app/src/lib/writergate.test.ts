// Round 12 (audit 2026-11-23 fix design 1, HUNT-1103, ERR-1101, UI-1101):
// listeners outside the editor's DOM refusal (App's Ctrl/Cmd+, , the draw
// bar's keys, the object bars' PageUp / PageDown, dialogs, the context
// menu, toast actions) ask the module whether a writer of the job is out:
// a drag or a core action (Revert, Revert all, the offer put-back). Its own
// file: sessions other tests leave open would count here.
import { describe, expect, it } from "vitest";
import { CoreAction, DragSession, anyDragOpen, anyWriterOpen, gatedKeys } from "./dragsession";

function drag() {
  return new DragSession<number, number>({
    pause: () => () => {},
    hold: (t) => ({ index: t, gone: false }) as never,
    snapshot: () => "{}",
    step: () => true,
    drop: () => {},
    restore: () => {},
  });
}

const key = (prevented = false) => ({ defaultPrevented: prevented }) as KeyboardEvent;

describe("anyWriterOpen (round 12, fix design 1)", () => {
  it("is true while a core action is out, and only a drag counts for anyDragOpen", async () => {
    expect(anyWriterOpen()).toBe(false);
    const action = new CoreAction(() => () => {});
    let answer!: () => void;
    const out = action.run(() => new Promise<void>((r) => (answer = r)));
    expect(anyWriterOpen()).toBe(true);
    expect(anyDragOpen()).toBe(false);
    answer();
    await out;
    expect(anyWriterOpen()).toBe(false);
  });

  it("is closed again after a failed call, and counts every action", async () => {
    const a = new CoreAction(() => () => {});
    const b = new CoreAction(() => () => {});
    let fail!: (e: unknown) => void;
    let ok!: () => void;
    const one = a.run(() => new Promise<void>((_, r) => (fail = r)));
    const two = b.run(() => new Promise<void>((r) => (ok = r)));
    fail(new Error("busy"));
    await one.catch(() => {});
    expect(anyWriterOpen()).toBe(true);
    ok();
    await two;
    expect(anyWriterOpen()).toBe(false);
  });

  it("counts a drag too", () => {
    const d = drag();
    d.arm(1, 0);
    expect(anyWriterOpen()).toBe(true);
    d.cancel();
    expect(anyWriterOpen()).toBe(false);
  });
});

describe("gatedKeys (round 12, fix design 1)", () => {
  it("passes a key only when no writer is out and no listener before took it", async () => {
    const seen: string[] = [];
    const on = gatedKeys(() => seen.push("ran"));
    on(key());
    expect(seen).toEqual(["ran"]);
    on(key(true));
    expect(seen).toEqual(["ran"]);
    const action = new CoreAction(() => () => {});
    let answer!: () => void;
    const out = action.run(() => new Promise<void>((r) => (answer = r)));
    on(key());
    expect(seen).toEqual(["ran"]);
    answer();
    await out;
    on(key());
    expect(seen).toEqual(["ran", "ran"]);
    const d = drag();
    d.arm(1, 0);
    on(key());
    expect(seen).toEqual(["ran", "ran"]);
    d.cancel();
  });
});
