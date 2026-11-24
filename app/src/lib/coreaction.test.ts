// Revert, Revert all and the put-back of the offer's figures are a short
// transaction like a drag (audit 2026-11-23 design A1, HUNT-1001, UX-1001,
// UX-1002, ERR-1004): while the core's call is out the editor is no writer
// of the job. One predicate (`closedForInput`) answers for the drag and
// for these actions at every point that refuses the user's input.
import { describe, expect, it } from "vitest";
import { CoreAction, DragSession, REFUSED_EVENTS, closedForInput, dropForeign, refuseInput } from "./dragsession";
import { JobEdits } from "./jobedits";
import type { Project } from "./types";

function setup() {
  const edits = new JobEdits(() => ({ dormers: [{ x: 0 }] }) as unknown as Project);
  const seen: boolean[] = [];
  const action = new CoreAction(() => edits.pause(), (on) => seen.push(on));
  const drag = new DragSession<number, number>({
    pause: () => edits.pause(),
    hold: (t) => edits.hold("dormers", t),
    snapshot: () => "{}",
    step: () => true,
    drop: () => {},
    restore: () => {},
  });
  return { edits, action, drag, seen };
}

describe("closedForInput (design A1)", () => {
  it("is closed while a drag is open or a core action is out, and only then", async () => {
    const { action, drag } = setup();
    expect(closedForInput(drag, action)).toBe(false);
    expect(drag.arm(1, 0)).toBe(true);
    expect(closedForInput(drag, action)).toBe(true);
    await drag.drop();
    expect(closedForInput(drag, action)).toBe(false);
    let answer!: () => void;
    const out = action.run(() => new Promise<void>((r) => (answer = r)));
    expect(action.active).toBe(true);
    expect(closedForInput(drag, action)).toBe(true);
    answer();
    await out;
    expect(action.active).toBe(false);
    expect(closedForInput(drag, action)).toBe(false);
  });

  it("holds the core's answers to the user's edits until the action is over, also when it fails", async () => {
    const { action, edits, seen } = setup();
    let fail!: (e: unknown) => void;
    const out = action.run(() => new Promise<void>((_, r) => (fail = r)));
    expect(edits.pausing).toBe(true);
    fail(new Error("busy"));
    await expect(out).rejects.toThrow("busy");
    expect(edits.pausing).toBe(false);
    expect(seen).toEqual([true, false]);
  });

  it("refuses a second action while one is out, and a drag press too", async () => {
    const { action, drag } = setup();
    let answer!: () => void;
    const first = action.run(() => new Promise<void>((r) => (answer = r)));
    let ran = false;
    await action.run(async () => {
      ran = true;
    });
    expect(ran).toBe(false);
    // The editor asks the predicate before arming a drag (Editor.svelte `armDrag`).
    expect(closedForInput(drag, action)).toBe(true);
    answer();
    await first;
    expect(action.active).toBe(false);
  });

  it("runs what waited for the action once it is over, the newest of each kind", async () => {
    const { action } = setup();
    const ran: string[] = [];
    action.defer("reload", () => ran.push("now"));
    expect(ran).toEqual(["now"]);
    let answer!: () => void;
    const out = action.run(() => new Promise<void>((r) => (answer = r)));
    action.defer("reload", () => ran.push("first"));
    action.defer("reload", () => ran.push("second"));
    expect(ran).toEqual(["now"]);
    answer();
    await out;
    expect(ran).toEqual(["now", "second"]);
  });
});

// The dock is not made inert during a review action: the pressed button
// keeps the keyboard's focus while it waits (UI-604); its input is
// swallowed instead.
describe("refuseInput (design A1, UI-604)", () => {
  class Node {
    listeners = new Map<string, (e: Event) => void>();
    addEventListener(type: string, fn: (e: Event) => void, capture: boolean) {
      expect(capture).toBe(true);
      this.listeners.set(type, fn);
    }
    removeEventListener(type: string) {
      this.listeners.delete(type);
    }
  }
  const event = () => {
    const e = { prevented: false, stopped: false, preventDefault: () => (e.prevented = true), stopImmediatePropagation: () => (e.stopped = true) };
    return e;
  };
  it("swallows presses, keys and typed text only while on", () => {
    const node = new Node();
    const a = refuseInput(node as unknown as HTMLElement, { on: false });
    for (const type of ["pointerdown", "mousedown", "click", "keydown", "beforeinput", "contextmenu"]) expect(REFUSED_EVENTS).toContain(type);
    const off = event();
    node.listeners.get("mousedown")!(off as unknown as Event);
    expect(off.prevented).toBe(false);
    a.update({ on: true });
    for (const type of REFUSED_EVENTS) {
      const e = event();
      node.listeners.get(type)!(e as unknown as Event);
      expect(e.prevented && e.stopped, type).toBe(true);
    }
    a.destroy();
    expect(node.listeners.size).toBe(0);
  });

  // UX-1802: Undo / Redo pressed in the dock while an Undo through the core
  // is out reach the editor's window handler, which queues them; every
  // other key and press is still swallowed.
  it("lets the events `pass` names through to the window, without the browser's own action", () => {
    const node = new Node();
    const pass = (e: Event) => (e as unknown as { key?: string }).key === "z";
    refuseInput(node as unknown as HTMLElement, { on: true, pass });
    const keyEvent = (key: string) => {
      const e = { key, prevented: false, stopped: false, preventDefault: () => (e.prevented = true), stopImmediatePropagation: () => (e.stopped = true) };
      return e;
    };
    const z = keyEvent("z");
    node.listeners.get("keydown")!(z as unknown as Event);
    expect([z.prevented, z.stopped]).toEqual([true, false]);
    const other = keyEvent("a");
    node.listeners.get("keydown")!(other as unknown as Event);
    expect([other.prevented, other.stopped]).toEqual([true, true]);
  });
});

// Round 12 (audit 2026-11-23 fix design 1, HUNT-1101, UI-1101, ERR-1105):
// a commit that reaches the editor while the call is out is refused
// (`foreign`), and leaving waits for the answer (`settled`).
describe("CoreAction.foreign and settled (round 12, fix design 1)", () => {
  it("refuses a write only while the call is out; strict throws", async () => {
    const product = new CoreAction(() => () => {});
    const strict = new CoreAction(() => () => {}, undefined, true);
    expect(product.foreign("commit")).toBe(false);
    expect(strict.foreign("commit")).toBe(false);
    let a!: () => void;
    let b!: () => void;
    const one = product.run(() => new Promise<void>((r) => (a = r)));
    const two = strict.run(() => new Promise<void>((r) => (b = r)));
    expect(product.foreign("commit")).toBe(true);
    expect(() => strict.foreign("commit")).toThrow(/core action: foreign commit/);
    a();
    b();
    await one;
    await two;
    expect(product.foreign("commit")).toBe(false);
    expect(strict.foreign("undo step")).toBe(false);
  });

  it("settled resolves at once when no call is out, else once it ended, also when it failed", async () => {
    const action = new CoreAction(() => () => {});
    let early = false;
    await action.settled().then(() => (early = true));
    expect(early).toBe(true);
    let fail!: (e: unknown) => void;
    const out = action.run(() => new Promise<void>((_, r) => (fail = r)));
    let done = false;
    const waited = action.settled().then(() => (done = true));
    await Promise.resolve();
    expect(done).toBe(false);
    fail(new Error("busy"));
    await expect(out).rejects.toThrow("busy");
    await waited;
    expect(done).toBe(true);
    expect(action.active).toBe(false);
  });
});

// Round 13 (audit 2026-11-23 design A, TEST-1202 c): the release branch of
// the backstop. E2E runs the dev build, where `foreign` throws; this is the
// product path the editor's commit and undo step take.
describe("dropForeign, the backstop's release path (round 13, design A)", () => {
  it("passes a write while no call is out, and drops it with one notice while one is", async () => {
    const action = new CoreAction(() => () => {});
    let putBack = 0;
    let told = 0;
    const drop = () => dropForeign(action, "commit", () => putBack++, () => told++);
    expect(drop()).toBe(false);
    expect([putBack, told]).toEqual([0, 0]);
    let answer!: () => void;
    const out = action.run(() => new Promise<void>((r) => (answer = r)));
    expect(drop()).toBe(true);
    expect([putBack, told], "put back to the newest step, told once").toEqual([1, 1]);
    answer();
    await out;
    expect(drop()).toBe(false);
    expect([putBack, told]).toEqual([1, 1]);
  });

  it("throws in strict mode and then neither puts back nor tells", async () => {
    const action = new CoreAction(() => () => {}, undefined, true);
    let calls = 0;
    let answer!: () => void;
    const out = action.run(() => new Promise<void>((r) => (answer = r)));
    expect(() => dropForeign(action, "undo step", () => calls++, () => calls++)).toThrow(/foreign undo step/);
    expect(calls).toBe(0);
    answer();
    await out;
  });
});
