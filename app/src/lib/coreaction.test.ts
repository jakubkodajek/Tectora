// Revert, Revert all and the put-back of the offer's figures are a short
// transaction like a drag (audit 2026-11-23 design A1, HUNT-1001, UX-1001,
// UX-1002, ERR-1004): while the core's call is out the editor is no writer
// of the job. One predicate (`closedForInput`) answers for the drag and
// for these actions at every point that refuses the user's input.
import { describe, expect, it } from "vitest";
import { CoreAction, DragSession, closedForInput } from "./dragsession";
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
