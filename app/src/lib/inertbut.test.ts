// TEST-705 (E7): while a drag is open the editor closes its bars with
// `inertBut`; a bar that appears later in the drag (the selection the press
// made shows another bar) must be closed too. Without the observer a new
// child stays open. No DOM in these tests: a small stand-in.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { inertBut } from "./dragsession";

class El {
  inert = false;
  classList: { contains: (c: string) => boolean };
  constructor(cls: string) {
    this.classList = { contains: (c: string) => c === cls };
  }
}

/** Observers that fire only for the nodes they observe, as the browser's do. */
const observed: { node: unknown; run: () => void }[] = [];
class Observer {
  constructor(private run: () => void) {}
  observe(node: unknown, opts: { childList?: boolean }) {
    if (opts.childList) observed.push({ node, run: this.run });
  }
  disconnect() {
    for (let i = observed.length - 1; i >= 0; i--) if (observed[i].run === this.run) observed.splice(i, 1);
  }
}

function added(node: { children: El[] }, child: El) {
  node.children.push(child);
  for (const o of observed) if (o.node === node) o.run();
}

beforeEach(() => {
  vi.stubGlobal("HTMLElement", El);
  vi.stubGlobal("MutationObserver", Observer);
});
afterEach(() => {
  vi.unstubAllGlobals();
  observed.length = 0;
});

describe("inertBut", () => {
  it("closes every child but the kept one, also one added while it is on, and opens them again", () => {
    const plan = new El("plan");
    const bar = new El("bar");
    const node = { children: [plan, bar] };
    const action = inertBut(node as unknown as HTMLElement, { on: false, keep: "plan" });
    expect(bar.inert).toBe(false);
    action.update({ on: true, keep: "plan" });
    expect([plan.inert, bar.inert]).toEqual([false, true]);
    // The bar of the new selection appears during the drag.
    const later = new El("bar");
    added(node, later);
    expect(later.inert).toBe(true);
    action.update({ on: false, keep: "plan" });
    expect([plan.inert, bar.inert, later.inert]).toEqual([false, false, false]);
    action.destroy();
    expect(observed).toHaveLength(0);
  });

  // UI-1801: a review call keeps `.review` live and closes `.views`; when
  // the call ends `on` and `keep` flip in one update. Off means every child
  // is live again, whatever `keep` names.
  it("opens every child when it turns off, also when keep changes in the same update", () => {
    const views = new El("views");
    const review = new El("review");
    const node = { children: [views, review] };
    const action = inertBut(node as unknown as HTMLElement, { on: false, keep: "views" });
    action.update({ on: true, keep: "review" });
    expect([views.inert, review.inert]).toEqual([true, false]);
    action.update({ on: false, keep: "views" });
    expect([views.inert, review.inert]).toEqual([false, false]);
    // A child the observer reports after the end stays live too.
    const later = new El("bar");
    added(node, later);
    expect([views.inert, review.inert, later.inert]).toEqual([false, false, false]);
    action.destroy();
  });

  it("moves the live child when keep changes while it is on", () => {
    const views = new El("views");
    const review = new El("review");
    const node = { children: [views, review] };
    const action = inertBut(node as unknown as HTMLElement, { on: true, keep: "views" });
    expect([views.inert, review.inert]).toEqual([false, true]);
    action.update({ on: true, keep: "review" });
    expect([views.inert, review.inert]).toEqual([true, false]);
    action.destroy();
  });
});
