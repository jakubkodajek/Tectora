// HUNT-702: App.svelte's Ctrl/Cmd+, (settings) runs before the editor's key
// guard, so it asks the module whether any drag is open. Its own file: the
// sessions other tests leave open would count here.
import { describe, expect, it } from "vitest";
import { DragSession, anyDragOpen } from "./dragsession";

function session(slow = false) {
  let answer = () => {};
  const s = new DragSession<number, number>({
    pause: () => () => {},
    hold: (t) => ({ index: t, gone: false }) as never,
    snapshot: () => "{}",
    step: () => true,
    drop: () => (slow ? new Promise<void>((r) => (answer = r)) : undefined),
    restore: () => {},
  });
  return { s, answer: () => answer() };
}

describe("anyDragOpen", () => {
  it("is true from the press until the drop is in, and after a cancel false again", async () => {
    expect(anyDragOpen()).toBe(false);
    const a = session(true);
    a.s.arm(1, 0);
    expect(anyDragOpen()).toBe(true);
    a.s.step(1);
    const done = a.s.drop();
    expect(a.s.state).toBe("settling");
    expect(anyDragOpen()).toBe(true);
    a.answer();
    await done;
    expect(anyDragOpen()).toBe(false);
    const b = session();
    b.s.arm(1, 0);
    b.s.step(2);
    expect(anyDragOpen()).toBe(true);
    b.s.cancel();
    expect(anyDragOpen()).toBe(false);
  });

  it("counts every open session: one ending leaves the other open", () => {
    const a = session();
    const b = session();
    a.s.arm(1, 0);
    b.s.arm(2, 0);
    a.s.cancel();
    expect(anyDragOpen()).toBe(true);
    b.s.cancel();
    expect(anyDragOpen()).toBe(false);
  });
});
