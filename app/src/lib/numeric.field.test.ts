// The `use:numeric` action on one input reused for several items (a bar
// that switches chimneys with PageDown), and its marks and steps while
// answers are with the core: review round 5 UI-403, UX-401, UX-403.
// The tests run without a DOM, so the input is a small stand-in.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { numberLocale } from "./i18n.svelte";
import { STEP_WAIT_MS, formatNumber, numeric, type NumericOptions } from "./numeric.svelte";

/** A value as the field shows it in the UI language. */
const f = (v: number) => formatNumber(v, numberLocale());

type Listener = (e: any) => void;

class FakeInput {
  value = "";
  title = "";
  disabled = false;
  readOnly = false;
  type = "";
  inputMode = "";
  autocomplete = "";
  spellcheck = true;
  dataset: Record<string, string> = {};
  classList = { add() {} };
  parentElement = null;
  style: Record<string, string> = {};
  private attrs = new Map<string, string>();
  private listeners = new Map<string, Listener[]>();
  addEventListener(type: string, f: Listener) {
    this.listeners.set(type, [...(this.listeners.get(type) ?? []), f]);
  }
  removeEventListener(type: string, f: Listener) {
    this.listeners.set(type, (this.listeners.get(type) ?? []).filter((g) => g !== f));
  }
  fire(type: string, e: Record<string, unknown> = {}) {
    for (const f of this.listeners.get(type) ?? []) f({ preventDefault() {}, stopPropagation() {}, ...e });
  }
  toggleAttribute(n: string, on: boolean) {
    if (on) this.attrs.set(n, "");
    else this.attrs.delete(n);
  }
  setAttribute(n: string, v: string) {
    this.attrs.set(n, v);
  }
  removeAttribute(n: string) {
    this.attrs.delete(n);
  }
  hasAttribute(n: string) {
    return this.attrs.has(n);
  }
  getAttribute(n: string) {
    return this.attrs.get(n) ?? null;
  }
  select() {}
  contains(n: unknown) {
    return n === this;
  }
  /** Types `text` and presses Enter, as a user does. */
  type_(text: string) {
    this.value = text;
    this.fire("input");
    this.fire("keydown", { key: "Enter" });
  }
  press(key: string) {
    this.fire("keydown", { key });
  }
}

/** A core call per value sent, answered by hand. */
function core() {
  const sent: string[] = [];
  const answers: ((r?: unknown) => void)[] = [];
  return {
    sent,
    answer: (r?: unknown) => answers.shift()?.(r),
    /** The field options for `item`, stored value `value`. */
    opts: (item: string, value: number, step = 0.05): NumericOptions => ({
      key: item,
      value,
      step,
      spinner: false,
      onchange: (v: number) => {
        sent.push(`${item}=${v}`);
        return new Promise((r) => answers.push(r));
      },
    }),
  };
}

const flush = () => new Promise((r) => setTimeout(r, 0));

beforeEach(() => {
  vi.stubGlobal(
    "MutationObserver",
    class {
      observe() {}
      disconnect() {}
    },
  );
  vi.stubGlobal("window", { addEventListener() {}, removeEventListener() {} });
});
afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe("one input for several items (UI-403)", () => {
  it("sends the same value typed on the next item while the first is with the core", () => {
    const c = core();
    const input = new FakeInput();
    const a = numeric(input as unknown as HTMLInputElement, c.opts("chimney:0", 0.6));
    input.type_(f(0.9));
    a.update(c.opts("chimney:1", 0.6));
    input.type_(f(0.9));
    expect(c.sent).toEqual(["chimney:0=0.9", "chimney:1=0.9"]);
    a.destroy();
  });

  it("one step on the next item of the same size goes one step from its own value", () => {
    const c = core();
    const input = new FakeInput();
    const a = numeric(input as unknown as HTMLInputElement, c.opts("window:0", 0.78));
    input.press("ArrowUp");
    a.update(c.opts("window:1", 0.78));
    input.press("ArrowUp");
    expect(c.sent).toEqual(["window:0=0.83", "window:1=0.83"]);
    expect(input.value).toBe(f(0.83));
    a.destroy();
  });

  it("an answer for the item before does not mark the field of the next one", async () => {
    const c = core();
    const input = new FakeInput();
    const a = numeric(input as unknown as HTMLInputElement, c.opts("chimney:0", 0.6));
    input.type_(f(0.9));
    a.update({ ...c.opts("chimney:1", 0.9), onchange: () => undefined });
    c.answer("refused");
    await flush();
    expect(input.getAttribute("aria-invalid")).toBeNull();
    expect(input.value).toBe(f(0.9));
    a.destroy();
  });

  it("the same item keeps its memory: the same value committed twice is sent once", () => {
    const c = core();
    const input = new FakeInput();
    const a = numeric(input as unknown as HTMLInputElement, c.opts("chimney:0", 0.6));
    input.type_(f(0.9));
    a.update(c.opts("chimney:0", 0.6));
    input.type_(f(0.9));
    expect(c.sent).toEqual(["chimney:0=0.9"]);
    a.destroy();
  });
});

describe("a refused edit stays visible (UX-401)", () => {
  it("keeps the mark of a value the editor refused later until the user edits the field again", async () => {
    vi.useFakeTimers();
    const c = core();
    const input = new FakeInput();
    const a = numeric(input as unknown as HTMLInputElement, c.opts("part:0", 3));
    input.type_(f(4.2));
    // An outside save re-renders the field before the answer comes.
    a.update(c.opts("part:0", 3));
    c.answer("The item changed meanwhile.");
    await vi.advanceTimersByTimeAsync(0);
    expect(input.value).toBe(f(3));
    expect(input.getAttribute("aria-invalid")).toBe("true");
    expect(input.title).toBe("The item changed meanwhile.");
    await vi.advanceTimersByTimeAsync(10_000);
    a.update(c.opts("part:0", 3.5));
    expect(input.getAttribute("aria-invalid")).toBe("true");
    input.value = "4";
    input.fire("input");
    expect(input.getAttribute("aria-invalid")).toBeNull();
    a.destroy();
  });

  it("a value out of range is marked only for a moment", async () => {
    vi.useFakeTimers();
    const c = core();
    const input = new FakeInput();
    const a = numeric(input as unknown as HTMLInputElement, { ...c.opts("part:0", 30), min: 2, max: 90 });
    input.type_("500");
    expect(input.getAttribute("aria-invalid")).toBe("true");
    await vi.advanceTimersByTimeAsync(3000);
    expect(input.getAttribute("aria-invalid")).toBeNull();
    a.destroy();
  });
});

describe("a burst of steps shows the newest step (UX-403)", () => {
  it("does not show an earlier step coming back while later ones are out", async () => {
    const c = core();
    const input = new FakeInput();
    let o = c.opts("part:0", 3.4);
    const a = numeric(input as unknown as HTMLInputElement, o);
    const seen: string[] = [];
    for (let i = 0; i < 3; i++) {
      input.press("ArrowUp");
      seen.push(input.value);
    }
    // The first two steps come back one by one.
    for (const v of [3.45, 3.5]) {
      c.answer();
      await flush();
      o = { ...o, value: v };
      a.update(o);
      seen.push(input.value);
    }
    c.answer();
    await flush();
    a.update({ ...o, value: 3.55 });
    seen.push(input.value);
    expect(seen).toEqual([f(3.45), f(3.5), f(3.55), f(3.55), f(3.55), f(3.55)]);
    expect(c.sent).toEqual(["part:0=3.45", "part:0=3.5", "part:0=3.55"]);
    a.destroy();
  });

  it("shows the stored value once the burst is over and its last step was refused", async () => {
    const c = core();
    const input = new FakeInput();
    const o = c.opts("part:0", 3.4);
    const a = numeric(input as unknown as HTMLInputElement, o);
    input.press("ArrowUp");
    c.answer("refused");
    await flush();
    a.update(o);
    expect(input.value).toBe(f(3.4));
    a.destroy();
  });
});

describe("round 6: the numeric branches without a case (TEST-502)", () => {
  const timers = () => vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "Date", "performance"] });

  it("a refusal still marked on item A is cleared when the field switches to item B (N5)", async () => {
    const c = core();
    const input = new FakeInput();
    input.title = "Building height";
    const a = numeric(input as unknown as HTMLInputElement, c.opts("part:0", 3));
    input.type_(f(4.2));
    c.answer("stale");
    await flush();
    expect(input.getAttribute("aria-invalid")).toBe("true");
    expect(input.title).toBe("stale");
    a.update(c.opts("part:1", 3));
    expect(input.getAttribute("aria-invalid")).toBeNull();
    expect(input.title).toBe("Building height");
    a.destroy();
  });

  it("a burst whose last answer takes longer than STEP_WAIT_MS keeps showing the newest step (N9)", async () => {
    timers();
    const c = core();
    const input = new FakeInput();
    const o = c.opts("part:0", 3.4);
    const a = numeric(input as unknown as HTMLInputElement, o);
    for (let i = 0; i < 3; i++) input.press("ArrowUp");
    expect(c.sent).toEqual(["part:0=3.45", "part:0=3.5", "part:0=3.55"]);
    c.answer();
    await vi.advanceTimersByTimeAsync(0);
    a.update({ ...o, value: 3.45 });
    expect(input.value).toBe(f(3.55));
    await vi.advanceTimersByTimeAsync(2000);
    a.update({ ...o, value: 3.45 });
    expect(input.value).toBe(f(3.55));
    a.destroy();
  });

  it("a value typed after two quick steps is what the field shows when a step answer comes back (N12)", async () => {
    timers();
    const c = core();
    const input = new FakeInput();
    const o = c.opts("part:0", 3.4);
    const a = numeric(input as unknown as HTMLInputElement, o);
    input.press("ArrowUp");
    input.press("ArrowUp");
    input.type_(f(5));
    expect(c.sent).toEqual(["part:0=3.45", "part:0=3.5", "part:0=5"]);
    // The steps come back one by one, the editor re-renders the field with each.
    for (const v of [3.45, 3.5]) {
      c.answer();
      await vi.advanceTimersByTimeAsync(0);
      a.update({ ...o, value: v });
      expect(input.value).toBe(f(5));
    }
    c.answer();
    await vi.advanceTimersByTimeAsync(0);
    a.update({ ...o, value: 5 });
    expect(input.value).toBe(f(5));
    a.destroy();
  });

  it("once the burst is over the field shows what is stored, also when no answer changes it (N11)", async () => {
    timers();
    const c = core();
    const input = new FakeInput();
    const o = c.opts("part:0", 3.4);
    const a = numeric(input as unknown as HTMLInputElement, o);
    input.press("ArrowUp");
    // The core took the step but stored the value it had (its own rounding).
    c.answer();
    await vi.advanceTimersByTimeAsync(0);
    a.update({ ...o });
    expect(input.value).toBe(f(3.45));
    await vi.advanceTimersByTimeAsync(STEP_WAIT_MS + 100);
    expect(input.value).toBe(f(3.4));
    a.destroy();
  });

  it("the last answer of a slow burst shows the stored value when it settles (N14)", async () => {
    timers();
    const c = core();
    const input = new FakeInput();
    const o = c.opts("part:0", 3.4);
    const a = numeric(input as unknown as HTMLInputElement, o);
    input.press("ArrowUp");
    await vi.advanceTimersByTimeAsync(STEP_WAIT_MS + 500);
    // Still out: the newest step is shown.
    expect(input.value).toBe(f(3.45));
    c.answer();
    await vi.advanceTimersByTimeAsync(0);
    expect(input.value).toBe(f(3.4));
    a.destroy();
  });

  it("a typed value the core stores differently shows the stored value once its answer settles (TEST-606 N14b)", async () => {
    const c = core();
    const input = new FakeInput();
    const o = c.opts("part:0", 3.4);
    const a = numeric(input as unknown as HTMLInputElement, o);
    // Typed 5; the core stores 4.95 (rounded, clamped): the editor applies
    // that before the call settles, then the answer comes.
    input.type_(f(5));
    expect(c.sent).toEqual(["part:0=5"]);
    expect(input.value).toBe(f(5));
    a.update({ ...o, value: 4.95 });
    c.answer();
    await flush();
    expect(input.value).toBe(f(4.95));
    a.destroy();
  });
});
