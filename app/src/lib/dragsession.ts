// A drag of an object, a building or a corner in the plan is one transaction
// with one owner, from the press until its result is in the history
// (jobedits.ts, rule 7). Rounds of fixes kept the state of a drag in nine
// variables of two components, each guard reading another subset of them
// (ERR-501, ERR-502, UI-502, UI-601, UI-603, UI-605, ERR-601, ERR-603); here
// it is one state, and every way out goes through one `finish`.
//
//   idle -> armed (pressed) -> moving (past the slop) -> settling (the drop
//   is with the core) -> idle
//
// While the session is open (`active`), the drag is the only writer of the
// job:
// 1. User input is refused by the editor (keys, buttons, fields, other
//    pointers); only Escape (`cancel`) and F3 pass. Nothing is replayed.
// 2. Events that are not the user's (answers of the core: the jobedits
//    pause taken at `arm`; outside saves and follow mode: `defer`) wait for
//    the end of the transaction.
// 3. Undo / Redo pressed while the drop settles wait for `settled()`.
// A write that slips through anyway (`foreign`) throws when `strict` (tests,
// the dev build) and else ends the drag where the job is now, so a step of
// the drag never rebuilds the job from before that write.
//
// No roof logic here: the editor is the `DragHost`.

import type { Hold } from "./jobedits";

export type DragState = "idle" | "armed" | "moving" | "settling";

/** The drag under way, as the host sees it at a step, the drop and a cancel. */
export interface Drag<T, S> {
  /** What was pressed. */
  readonly target: T;
  /** The job's text when the press came: every step maps from it. */
  readonly base: string;
  /** Where the newest step put the target (null before the first). */
  readonly last: S | null;
}

/** What the session needs from the editor. */
export interface DragHost<T, S> {
  /** Holds back the answers of the core (JobEdits.pause); the returned function ends it. */
  pause(): () => void;
  /** Holds the item the drag is for (JobEdits.hold: the item, not its index); null for a target that cannot be dragged. */
  hold(target: T): Hold | null;
  /** The job's text now. */
  snapshot(): string;
  /** One step: moves the target to `to`. False when that changed nothing. */
  step(drag: Drag<T, S>, to: S): boolean;
  /**
   * The release after at least one step: puts the result into the history.
   * A promise when the core finishes the move (the session is `settling`
   * until it settles); its later writes go through `DragSession.write`.
   */
  drop(drag: Drag<T, S>): void | Promise<void>;
  /** Escape: the job goes back to `base`; nothing is saved. */
  restore(base: string): void;
  /** The state changed (for what the editor shows). */
  changed?(state: DragState): void;
}

/** Sessions with a drag open, across components. */
const open = new Set<object>();

/**
 * A drag is open in some plan. App-wide shortcuts outside the editor
 * (Ctrl/Cmd+, opens the settings) check it: their listener runs before the
 * editor's refusal (HUNT-702).
 */
export function anyDragOpen(): boolean {
  return open.size > 0;
}

/** Core actions (Revert, Revert all, the offer put-back) with their call out, across components. */
const actions = new Set<object>();

/**
 * A writer of the job is out: a drag, or a core action whose call is out.
 * The one gate of everything outside the editor's DOM refusal (round 12,
 * audit 2026-11-23 fix design 1: HUNT-1103, ERR-1101, UI-1101): window
 * keys (App's Ctrl/Cmd+, , the draw bar, the object bars' PageUp /
 * PageDown, dialogs, the context menu), the window-level presses that
 * commit a field, and toast actions. `gate.scan.test.ts` lists every such
 * entry point; a new one fails it until it is gated or marked passive.
 */
export function anyWriterOpen(): boolean {
  return open.size > 0 || actions.size > 0;
}

/**
 * Wraps a window-level key handler: the key does nothing while a writer
 * of the job is out, or when a listener before it took the key already
 * (the editor's refusal calls `preventDefault`).
 */
export function gatedKeys<E extends { defaultPrevented: boolean }>(handler: (e: E) => void): (e: E) => void {
  return (e: E) => {
    if (e.defaultPrevented || anyWriterOpen()) return;
    handler(e);
  };
}

export class DragSession<T, S> {
  private current: DragState = "idle";
  private pointerId: number | null = null;
  private drag: { target: T; base: string; last: S | null } | null = null;
  private held: Hold | null = null;
  private heldAt = -1;
  private resume: (() => void) | null = null;
  /** Writes of the drag's own under way (`write`). */
  private own = 0;
  private deferred = new Map<string, () => void>();
  private waiting: (() => void)[] = [];

  /** `strict`: a foreign write while the session is open throws (tests, the dev build). */
  constructor(
    private host: DragHost<T, S>,
    private strict = false,
  ) {}

  get state(): DragState {
    return this.current;
  }

  /** A drag is open: from the press until its result is in the history. */
  get active(): boolean {
    return this.current !== "idle";
  }

  /** The pointer that holds the drag, or null. */
  get pointer(): number | null {
    return this.pointerId;
  }

  private set(state: DragState) {
    if (this.current === state) return;
    this.current = state;
    if (state === "idle") open.delete(this);
    else open.add(this);
    this.host.changed?.(state);
  }

  /**
   * A press on something that can be dragged. Takes the pause of the core's
   * answers and holds the item. False when a drag is open already (a second
   * pointer, a press while the last drop settles) or there is nothing to
   * hold: the press must then start no drag.
   */
  arm(pointerId: number, target: T): boolean {
    if (this.current !== "idle") return false;
    const hold = this.host.hold(target);
    if (!hold || hold.gone) return false;
    this.held = hold;
    this.heldAt = hold.index;
    this.pointerId = pointerId;
    this.drag = { target, base: this.host.snapshot(), last: null };
    this.resume = this.host.pause();
    this.set("armed");
    return true;
  }

  /** The held item is still the pressed one, where it was pressed. */
  private same(): boolean {
    return !!this.held && !this.held.gone && this.held.index === this.heldAt;
  }

  /**
   * A step of the pointer. False when no drag is open (cancelled, cut, or
   * never armed) or the step changed nothing. An item that is no longer the
   * pressed one ends the drag: the step never moves what now has its index.
   */
  step(to: S): boolean {
    if ((this.current !== "armed" && this.current !== "moving") || !this.drag) return false;
    if (!this.same()) {
      this.finish();
      return false;
    }
    const drag = this.drag;
    const moved = this.write(() => this.host.step(drag, to));
    if (!moved) return false;
    drag.last = to;
    this.set("moving");
    return true;
  }

  /**
   * The release (also a lost pointer, a plan that went away). Pressed and
   * never moved: nothing happened. Moved: the host's drop; the session
   * stays `settling` until that is in the history. Resolves when the
   * session is idle again. What a synchronous drop throws is thrown here,
   * after the session ended.
   */
  drop(): Promise<void> {
    if (this.current === "armed") {
      this.finish();
      return Promise.resolve();
    }
    if (this.current !== "moving" || !this.drag) return this.settled();
    if (!this.same()) {
      this.finish();
      return Promise.resolve();
    }
    const drag = this.drag;
    this.set("settling");
    let going: void | Promise<void>;
    try {
      going = this.write(() => this.host.drop(drag));
    } catch (e) {
      this.finish();
      throw e;
    }
    if (!(going instanceof Promise)) {
      this.finish();
      return Promise.resolve();
    }
    return going.then(
      () => this.finish(),
      (e) => {
        this.finish();
        throw e;
      },
    );
  }

  /**
   * Ends the drag where it is, as its release would (the job is being
   * left): the plan's further steps of it find no session and do nothing.
   */
  cut(): Promise<void> {
    return this.drop();
  }

  /**
   * Escape: the job goes back to where the drag began, nothing is saved.
   * False when there is nothing to cancel (no drag, or the drop is already
   * with the core).
   */
  cancel(): boolean {
    if ((this.current !== "armed" && this.current !== "moving") || !this.drag) return false;
    const { base } = this.drag;
    const moved = this.current === "moving";
    try {
      if (moved) this.write(() => this.host.restore(base));
    } finally {
      this.finish();
    }
    return true;
  }

  /** Runs a write of the drag's own (a step, the drop's commit after the core answered). */
  write<R>(run: () => R): R {
    this.own++;
    try {
      return run();
    } finally {
      this.own--;
    }
  }

  /**
   * Called by the editor before every write of the job (commit, undo step,
   * swap from outside). A write that is not the drag's own while the
   * session is open breaks the rule: throws when `strict`; else a drag
   * still held ends where the job is now (its base is older than this
   * write), and a drop that settles is left to give way itself.
   */
  foreign(what = "write") {
    if (this.current === "idle" || this.own > 0) return;
    if (this.strict) throw new Error(`drag session: foreign ${what} while ${this.current}`);
    if (this.current !== "settling") this.finish();
  }

  /**
   * Something that is not the user's (a reload after an outside save)
   * waits for the end of the transaction; one run per `key`, the newest
   * wins. Runs at once when no drag is open.
   */
  defer(key: string, run: () => void) {
    if (this.current === "idle") run();
    else this.deferred.set(key, run);
  }

  /** Resolves once no drag is open (at once when none is). Never rejects. */
  settled(): Promise<void> {
    if (this.current === "idle") return Promise.resolve();
    return new Promise((r) => this.waiting.push(r));
  }

  /** The one way out: the pause ends, the base is gone, what waited runs. */
  private finish() {
    const resume = this.resume;
    const deferred = [...this.deferred.values()];
    const waiting = this.waiting;
    this.resume = null;
    this.drag = null;
    this.held = null;
    this.heldAt = -1;
    this.pointerId = null;
    this.deferred = new Map();
    this.waiting = [];
    try {
      this.set("idle");
    } finally {
      resume?.();
      for (const r of waiting) r();
      for (const run of deferred) {
        try {
          run();
        } catch (e) {
          console.error(e);
        }
      }
    }
  }
}

/**
 * Revert, Revert all and the put-back of the offer's figures: a short
 * transaction of the core's that replaces the job, like a drag (audit
 * 2026-11-23 design A1, owner decision; HUNT-1001, UX-1001, UX-1002,
 * ERR-1004). While the call is out the editor is no writer of the job:
 * 1. User input is refused at the points that refuse it during a drag
 *    (`closedForInput`): keys, fields, bar buttons, delete, drag presses.
 *    Nothing is replayed; the call takes at most the core's lock wait
 *    (about 3 s, then it answers `busy` and changed nothing).
 * 2. The core's answers to edits made before (JobEdits.pause) and reloads
 *    after outside saves (`defer`) wait for the end of the transaction.
 * 3. The answer replaces the job as it is: there is nothing local to
 *    merge into it.
 * A second action while one is out does nothing.
 * 4. A write that reaches the editor anyway (`foreign`: a commit or an
 *    undo step while the call is out) is refused: it throws when `strict`
 *    (tests, the dev build), else the editor drops it and says so. The
 *    gates before it (the prepare phase, `anyWriterOpen`) should leave
 *    nothing for it (round 12, fix design 1).
 */
export class CoreAction {
  private resume: (() => void) | null = null;
  private deferred = new Map<string, () => void>();
  private waiting: (() => void)[] = [];

  /** `pause`: JobEdits.pause; `changed`: the action began (true) or ended (false), for what the editor shows; `strict`: a foreign write throws. */
  constructor(
    private pause: () => () => void,
    private changed?: (active: boolean) => void,
    private strict = false,
  ) {}

  /** The call is out. */
  get active(): boolean {
    return this.resume !== null;
  }

  /** Runs `work` as the transaction; resolves to undefined without running it when one is out already. Rethrows what `work` throws, after the end. */
  async run<R>(work: () => Promise<R>): Promise<R | undefined> {
    if (this.resume) return undefined;
    this.resume = this.pause();
    actions.add(this);
    this.changed?.(true);
    try {
      return await work();
    } finally {
      const resume = this.resume;
      const deferred = [...this.deferred.values()];
      const waiting = this.waiting;
      this.resume = null;
      this.deferred = new Map();
      this.waiting = [];
      actions.delete(this);
      try {
        this.changed?.(false);
      } finally {
        resume?.();
        for (const r of waiting) r();
        for (const run of deferred) {
          try {
            run();
          } catch (e) {
            console.error(e);
          }
        }
      }
    }
  }

  /**
   * Called by the editor before every write of the job of its own (commit,
   * undo step). True when the write must be refused: the call is out and
   * its answer will replace the job. Throws instead when `strict`.
   */
  foreign(what = "write"): boolean {
    if (!this.resume) return false;
    if (this.strict) throw new Error(`core action: foreign ${what} while the call is out`);
    return true;
  }

  /** Resolves once no call is out (at once when none is). Never rejects. */
  settled(): Promise<void> {
    if (!this.resume) return Promise.resolve();
    return new Promise((r) => this.waiting.push(r));
  }

  /** Something that is not the user's waits for the end of the transaction; one run per `key`, the newest wins. At once when none is out. */
  defer(key: string, run: () => void) {
    if (!this.resume) run();
    else this.deferred.set(key, run);
  }
}

/**
 * The product path of the backstop (`CoreAction.foreign`): a write of the
 * editor's own that reached it while the call is out is dropped. `putBack`
 * returns the job to the newest step, `tell` says so once. True when the
 * write was dropped; throws when the action is `strict` (round 13 design A,
 * TEST-1202: the release branch gets a test without a release build).
 */
export function dropForeign(action: CoreAction, what: string, putBack: () => void, tell: () => void): boolean {
  if (!action.foreign(what)) return false;
  putBack();
  tell();
  return true;
}

/**
 * The editor is closed for the user's input: a drag is open or a core
 * action is out. The one predicate every refusal asks (keys, fields, bar
 * buttons, delete, drag presses, the autosave's timer).
 */
export function closedForInput(...writers: readonly { readonly active: boolean }[]): boolean {
  return writers.some((w) => w.active);
}

/** The user's input events a closed part of the editor swallows (`refuseInput`). */
export const REFUSED_EVENTS = ["pointerdown", "mousedown", "mouseup", "click", "dblclick", "contextmenu", "touchstart", "keydown", "keypress", "beforeinput", "paste", "drop"] as const;

/**
 * Svelte action: while `on`, the user's input in the node is swallowed in
 * the capture phase (no press, no focus, no key, no typed text), and
 * unlike `inert` the focus stays where it is: the dock during a review
 * action, whose pressed button keeps the keyboard's focus while it waits
 * (UI-604, design A1).
 */
export function refuseInput(node: HTMLElement, opts: { on: boolean }) {
  let now = opts;
  const swallow = (e: Event) => {
    if (!now.on) return;
    e.preventDefault();
    e.stopImmediatePropagation();
  };
  for (const type of REFUSED_EVENTS) node.addEventListener(type, swallow, true);
  return {
    update(next: { on: boolean }) {
      now = next;
    },
    destroy() {
      for (const type of REFUSED_EVENTS) node.removeEventListener(type, swallow, true);
    },
  };
}

/**
 * Svelte action: while `on`, every child of the node except the one with
 * class `keep` is `inert` (no click, no focus, no key reaches it), also
 * children added meanwhile (the bar under the plan changes with the
 * selection the press made). The editor closes its bars with it while a
 * drag is open; the plan (`keep`) stays live for the drag itself.
 */
export function inertBut(node: HTMLElement, opts: { on: boolean; keep: string }) {
  let now = opts;
  const apply = () => {
    // Every child is set each time: off opens all of them whatever `keep`
    // names (a review call flips `on` and `keep` in one update, UI-1801),
    // and a change of `keep` while on moves the live child.
    for (const el of node.children) if (el instanceof HTMLElement) el.inert = now.on && !el.classList.contains(now.keep);
  };
  const seen = new MutationObserver(apply);
  seen.observe(node, { childList: true });
  apply();
  return {
    update(next: { on: boolean; keep: string }) {
      now = next;
      apply();
    },
    destroy() {
      seen.disconnect();
    },
  };
}
