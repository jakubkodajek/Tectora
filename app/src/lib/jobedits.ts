// One rule for every edit of the job that waits for the core (a typed
// building height, a dormer width the core checks, Duplicate, a status
// change that stores the result, …). Rounds of fixes compared a copy of the
// item taken before the await with whatever was at its index after it; that
// took the user's own earlier step, or the same item read back from the file
// in the core's form, for "an AI change", and took the neighbour at the same
// index for the item itself (ERR-301, ERR-302, UI-301). The rule here:
//
// 1. What an edit is for is *held* at input time (`hold`, `holdRoof`): a
//    live handle, not an index and not a JSON string.
// 2. Only the place where the job is replaced from outside (reload after an
//    AI save, merge on a save conflict, Revert) decides what that save did,
//    by comparing the file before and after it (`outside`). Both are written
//    by the core, so they compare as text; an editor copy is never compared
//    with a file copy. A held item that the save left alone stays held and
//    follows its item to a new index; one it changed or removed is `gone`.
// 3. The user's own edits never kill a hold, they only move it (`own`).
//    Only when the user's own step removed the item itself (a delete, the
//    undo of its add) the hold ends, and it ends quietly (`removed`): the
//    user knows, there is nothing to report (UX-402).
// 4. Edits are serialized (`run`): each asks the core for the job as the
//    one before left it, and is applied only if nothing touched the job
//    while the core answered; else the answer is dropped and asked again.
// 5. While the user drags (`pause`), no answer is applied: a drag rebuilds
//    the job from where it started at every step, so an answer applied
//    meanwhile would be written back to the old value on release (ERR-401).
//    The edit is asked again once the drag has ended, on the dragged job.
// 6. A refusal is always heard (`refused`, with the hold: the editor names
//    the object in its note); a quiet end is only `removed` and `closed`.
//
// No roof logic here: only which object an edit belongs to.

import { t } from "./i18n.svelte";
import { roofInput } from "./offer";
import { partLabel } from "./partname";
import type { Project } from "./types";

/** The lists of a job whose items an edit can be for. */
export type ListName = "parts" | "dormers" | "chimneys" | "windows" | "roof_openings" | "manual_faces" | "manual_lines";
export const LISTS: readonly ListName[] = ["parts", "dormers", "chimneys", "windows", "roof_openings", "manual_faces", "manual_lines"];

const key = (v: unknown) => JSON.stringify(v ?? null);

/**
 * Where item `index` of list `from` is in list `to`, both copies of the
 * same list in the same form (two files, or two editor copies): the same
 * index when the item is there unchanged; else the place of the same item
 * among its equals (the n-th of them stays the n-th), which follows it past
 * items removed or added before it. Null when it is not in `to` unchanged,
 * or cannot be told from its equals.
 */
export function follow(from: readonly unknown[] | null | undefined, to: readonly unknown[] | null | undefined, index: number): number | null {
  if (!from || !to || index < 0 || index >= from.length) return null;
  const k = key(from[index]);
  if (index < to.length && key(to[index]) === k) return index;
  const twins = (list: readonly unknown[]) => list.flatMap((v, i) => (key(v) === k ? [i] : []));
  const a = twins(from);
  const b = twins(to);
  if (a.length !== b.length) return null;
  return b[a.indexOf(index)] ?? null;
}

/**
 * A held item: `index` is where it is now; `gone` once an outside save
 * changed or removed it, or the user's own step removed it (then also
 * `removed`).
 */
export interface Hold {
  readonly list: ListName;
  index: number;
  gone: boolean;
  /** The user's own step removed the item (a delete, an undo): an edit for it ends quietly. */
  removed?: boolean;
  /** A selection, not an edit: it stays on an item an outside save changed in place. */
  readonly loose: boolean;
}

/** The held roof input (everything the result is calculated from); `changed` once it is another roof. */
export interface RoofHold {
  text: string;
  changed: boolean;
}

/** How a queued edit ended. */
export type Outcome =
  /** Applied. */
  | "done"
  /** An outside save changed or removed the item meanwhile: refused. */
  | "gone"
  /** The job kept changing while the core answered: refused. */
  | "restless"
  /** The user's own step removed the item meanwhile: not made, nothing to report (UX-402). */
  | "removed"
  /** No job any more (the editor closed). */
  | "closed";

/** Why an edit that ended as `o` was not made, for its field's tooltip; undefined when it was made. */
export function notMade(o: Outcome): string | undefined {
  return o === "done" ? undefined : o === "gone" ? t("editor.actionStale") : o === "restless" ? t("editor.actionBusy") : o === "removed" ? t("editor.actionRemoved") : t("num.refused");
}

const ITEM_NAME: Record<Exclude<ListName, "parts">, string> = {
  dormers: "dormer.n",
  chimneys: "opening.chimneyN",
  windows: "opening.windowN",
  roof_openings: "opening.holeN",
  manual_lines: "manual.lineN",
  manual_faces: "manual.faceN",
};

/**
 * The held item's name for a note ("Komín 2", "Budova „Garáž“"): a
 * building still held by its own name in `job`, anything else (and a
 * building an outside save took away) by its number (UX-401, UX-404).
 */
export function heldName(h: Hold, job: Project | null): string {
  if (h.list === "parts") return h.gone ? t("prob.item.part", { n: h.index + 1 }) : partLabel(job?.parts[h.index]?.name, h.index);
  return t(ITEM_NAME[h.list], { n: h.index + 1 });
}

/**
 * What the editor gives a bar for its core-backed edits: `ask` works out
 * the change of item `index` of `list` (held when this is called) on the
 * job as it is at its turn, `apply` stores it. Resolves to the reason when
 * the edit was not made, undefined when it was; rejects with what `ask` threw.
 */
export type BarEdit = <T>(list: ListName, index: number, ask: (job: Project, at: number) => Promise<T>, apply: (answer: T, at: number) => void) => Promise<string | undefined>;

/** How often an edit asks the core again because the job changed while it answered. */
export const TRIES = 4;

export class JobEdits {
  private holds = new Set<WeakRef<Hold>>();
  private roofs = new Set<WeakRef<RoofHold>>();
  private tail: Promise<unknown> = Promise.resolve();
  private waiting = 0;
  /** Gestures under way (`pause`) and the promise that resolves when the last ended. */
  private paused = 0;
  private resumed: Promise<void> | null = null;
  private wake: (() => void) | null = null;

  /**
   * `job` is the job the editor holds now; `stamp` its text (any change of
   * the job changes it); `refused` hears every edit that was refused, with
   * the hold it was for (null: an edit for no single item).
   */
  constructor(
    private job: () => Project | null,
    private stamp: () => string = () => JSON.stringify(this.job()),
    private refused: (o: "gone" | "restless", hold: Hold | null) => void = () => {},
  ) {}

  /**
   * A gesture that rebuilds the job from its start at every step (a drag)
   * begins: no answer is applied until the returned function is called
   * (once is enough; more calls do nothing). Answers that came meanwhile
   * are asked again on the job the gesture left (ERR-401).
   */
  pause(): () => void {
    if (this.paused++ === 0) this.resumed = new Promise((r) => (this.wake = r));
    let ended = false;
    return () => {
      if (ended) return;
      ended = true;
      if (--this.paused === 0) {
        const wake = this.wake;
        this.resumed = this.wake = null;
        wake?.();
      }
    };
  }

  /** A gesture is under way (`pause`). */
  get pausing(): boolean {
    return this.paused > 0;
  }

  /** Holds item `index` of `list` as what an edit made now is for. */
  hold(list: ListName, index: number, loose = false): Hold {
    const h: Hold = { list, index, gone: (this.job()?.[list] as unknown[] | undefined)?.[index] === undefined, loose };
    this.holds.add(new WeakRef(h));
    return h;
  }

  /** Holds the roof input as it is now. */
  holdRoof(): RoofHold {
    const p = this.job();
    const h: RoofHold = { text: p ? roofInput(p) : "", changed: !p };
    this.roofs.add(new WeakRef(h));
    return h;
  }

  /** The roof is still the held one: no outside save and no edit of the user changed its input. */
  roofSame(h: RoofHold): boolean {
    const p = this.job();
    return !!p && !h.changed && roofInput(p) === h.text;
  }

  private each<T extends object>(set: Set<WeakRef<T>>, f: (h: T) => void) {
    for (const ref of [...set]) {
      const h = ref.deref();
      if (h) f(h);
      else set.delete(ref);
    }
  }

  /**
   * An outside save replaced the job: `base` is the file as the editor last
   * read or wrote it, `remote` the file now (both in the core's form),
   * `before` the editor's job until now (with edits not saved yet) and
   * `after` the job it holds from now on. Call it right after the swap.
   */
  outside(base: Project, remote: Project, before: Project, after: Project) {
    this.each(this.holds, (h) => {
      if (h.gone) return;
      const b = base[h.list] ?? [];
      const r = remote[h.list] ?? [];
      const mine = before[h.list] ?? [];
      // The save left the whole list alone: the editor's list stays as it is.
      if (key(b) === key(r)) return;
      // An unsaved edit of the user changed the list's length: its indices
      // are not the file's, so the item cannot be followed.
      const at = mine.length === b.length ? follow(b, r, h.index) : null;
      if (at !== null) h.index = at;
      else if (h.loose && mine.length === b.length && r.length === b.length) return;
      else h.gone = true;
    });
    const roofChanged = roofInput(base) !== roofInput(remote);
    this.each(this.roofs, (h) => {
      if (h.changed) return;
      // An edit of the user's own since the hold, or the outside save, made it another roof.
      if (roofChanged || roofInput(before) !== h.text) h.changed = true;
      // The same roof, now in the form the file has it.
      else h.text = roofInput(after);
    });
  }

  /**
   * An edit of the user's own changed the job from `before()` to `after`
   * (an undo step, a delete, a copy): holds follow their items when a
   * list's length changed; a hold whose item the step removed ends as
   * `removed` (quietly). `before` is only read while something is held.
   */
  own(before: () => Project, after: Project) {
    let b: Project | null = null;
    this.each(this.holds, (h) => {
      if (h.gone) return;
      b ??= before();
      const from = b[h.list] ?? [];
      const to = after[h.list] ?? [];
      if (from.length === to.length) return;
      const at = follow(from, to, h.index);
      if (at !== null) h.index = at;
      else {
        h.gone = true;
        // Removed by the step, not only lost among equal items: quiet.
        const k = key(from[h.index]);
        h.removed = !to.some((v) => key(v) === k);
      }
    });
  }

  /** Edits queued or running. */
  get size(): number {
    return this.waiting;
  }

  /** Resolves once no edit is queued or running (also ones queued while waiting). Never rejects. */
  async idle(): Promise<void> {
    while (this.waiting) await this.tail;
  }

  /**
   * Queues an edit of the held item behind the edits before it. `ask`
   * works out the change for `job` (the job as it is when the edit's turn
   * comes, never to be changed by `ask`) with the item at index `at`;
   * `apply` puts the answer into the job at `at`. An answer the job moved
   * away from is dropped and asked again. A refusal thrown by `ask`
   * reaches the caller. `hold` null: an edit for no single item.
   */
  run<T>(hold: Hold | null, ask: (job: Project, at: number) => Promise<T>, apply: (answer: T, at: number) => void): Promise<Outcome> {
    this.waiting++;
    const turn = this.tail.then(() => this.turn(hold, ask, apply));
    this.tail = turn.then(
      () => {},
      () => {},
    );
    return turn.finally(() => {
      this.waiting--;
    });
  }

  private async turn<T>(hold: Hold | null, ask: (job: Project, at: number) => Promise<T>, apply: (answer: T, at: number) => void): Promise<Outcome> {
    for (let tries = 0; tries < TRIES; ) {
      // Not while the user drags: asked once the drag has ended (ERR-401).
      while (this.resumed) await this.resumed;
      const job = this.job();
      if (!job) return "closed";
      if (hold?.gone) break;
      const asked = this.stamp();
      const answer = await ask(job, hold?.index ?? -1);
      if (!this.job()) return "closed";
      if (hold?.gone) break;
      // A drag began while the core answered: ask again after it, on the
      // job it leaves; that is no try of the job holding still.
      if (this.resumed) continue;
      if (this.stamp() === asked) {
        apply(answer, hold?.index ?? -1);
        return "done";
      }
      tries++;
    }
    // The user's own step removed the item: he knows, nothing to say (UX-402).
    if (hold?.removed) return "removed";
    const o = hold?.gone ? "gone" : "restless";
    this.refused(o, hold);
    return o;
  }
}
