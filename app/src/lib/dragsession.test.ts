// The drag transaction (dragsession.ts, jobedits.ts rule 7): the state table,
// and seeded random sequences of everything that can meet a drag.
import { describe, expect, it } from "vitest";
import { DragSession, type Drag, type DragState } from "./dragsession";
import { JobEdits, type Hold } from "./jobedits";
import type { Project } from "./types";

interface Job {
  dormers: { id: number; x: number }[];
  notes: string;
}

/** A small editor around the session: a job, its history, the core-backed edits, reloads. */
class Ed {
  job: Job = { dormers: [0, 1, 2].map((id) => ({ id, x: id * 10 })), notes: "" };
  history: string[] = [JSON.stringify(this.job)];
  states: DragState[] = [];
  edits = new JobEdits(() => this.job as unknown as Project);
  /** Resolvers of drops that wait for the core (`slow`). */
  drops: (() => void)[] = [];
  slow = false;
  failDrop = false;
  reloads = 0;
  reloadsDue = 0;
  private reloadWaits = false;
  steps = 0;
  undone = 0;
  session: DragSession<number, number>;

  constructor(strict = true) {
    this.session = new DragSession<number, number>(
      {
        pause: () => this.edits.pause(),
        hold: (target): Hold | null => (target < 0 ? null : this.edits.hold("dormers", target)),
        snapshot: () => JSON.stringify(this.job),
        step: (drag, to) => {
          this.fresh(drag);
          if (to === 0) return false;
          const base = JSON.parse(drag.base) as Job;
          base.dormers[drag.target].x += to;
          this.job = base;
          this.steps++;
          return true;
        },
        drop: (drag) => {
          this.fresh(drag);
          if (this.failDrop) throw new Error("drop failed");
          if (!this.slow) return this.commit();
          return new Promise<void>((r) => this.drops.push(r)).then(() => this.session.write(() => this.commit()));
        },
        restore: (base) => {
          expect(base).toBe(this.history[this.history.length - 1]);
          this.job = JSON.parse(base) as Job;
        },
        changed: (s) => {
          this.states.push(s);
          if (s === "idle") this.reloadWaits = false;
        },
      },
      strict,
    );
  }

  /** The base a step or drop maps from is never older than the last commit. */
  private fresh(drag: Drag<number, number>) {
    expect(drag.base).toBe(this.history[this.history.length - 1]);
  }

  /** The editor's commit: the tripwire first, then the history. */
  commit() {
    this.session.foreign("commit");
    const s = JSON.stringify(this.job);
    if (this.history[this.history.length - 1] !== s) this.history.push(s);
  }

  /** A write that is not the drag's (what a missed guard would let through). */
  foreignCommit(note: string) {
    this.job.notes += note;
    this.commit();
  }

  /** An outside save of this job: the reload waits for the end of a drag. */
  outsideSave() {
    if (!this.session.active || !this.reloadWaits) this.reloadsDue++;
    if (this.session.active) this.reloadWaits = true;
    this.session.defer("reload", () => {
      this.reloads++;
    });
  }

  /** Undo as the editor does it: refused while the drag is held, queued behind a drop that settles. */
  undo(): boolean {
    if (this.session.state === "armed" || this.session.state === "moving") return false;
    void (async () => {
      // As the editor's `afterEdits`: until neither a drag nor an edit is under way.
      while (this.session.active || this.edits.size) {
        await this.session.settled();
        await this.edits.idle();
      }
      if (this.history.length > 1) {
        this.session.foreign("undo");
        this.history.pop();
        this.job = JSON.parse(this.history[this.history.length - 1]) as Job;
      }
      this.undone++;
    })();
    return true;
  }

  /** An edit that waits for the core; `answer()` lets the core answer. */
  coreEdit(index: number): () => void {
    let answer = () => {};
    const asked = new Promise<void>((r) => (answer = r));
    void this.edits.run(
      this.edits.hold("dormers", index),
      async () => {
        await asked;
        return 1;
      },
      (dx, at) => {
        expect(this.session.active).toBe(false);
        this.job.dormers[at].x += dx;
        this.commit();
      },
    );
    return answer;
  }
}

const flush = async () => {
  for (let i = 0; i < 12; i++) await Promise.resolve();
};

/** An editor whose session is in `state` (a slow drop left unanswered for `settling`). */
function inState(state: DragState, strict = true): Ed {
  const ed = new Ed(strict);
  if (state === "idle") return ed;
  expect(ed.session.arm(1, 1)).toBe(true);
  if (state === "armed") return ed;
  expect(ed.session.step(5)).toBe(true);
  if (state === "moving") return ed;
  ed.slow = true;
  void ed.session.drop();
  expect(ed.session.state).toBe("settling");
  return ed;
}

const X0 = [0, 10, 20];
const xs = (ed: Ed) => ed.job.dormers.map((d) => d.x);

describe("DragSession: state by event", () => {
  const STATES: DragState[] = ["idle", "armed", "moving", "settling"];

  it("arm: only from idle; a second arm while a drag is open is refused and changes nothing", () => {
    for (const s of STATES) {
      const ed = inState(s);
      const pointer = ed.session.pointer;
      expect(ed.session.arm(2, 2), s).toBe(s === "idle");
      expect(ed.session.state, s).toBe(s === "idle" ? "armed" : s);
      expect(ed.session.pointer, s).toBe(s === "idle" ? 2 : pointer);
      expect(ed.edits.pausing, s).toBe(true);
    }
  });

  it("arm: nothing to hold (no item there) is refused and takes no pause", () => {
    const ed = new Ed();
    expect(ed.session.arm(1, 7)).toBe(false);
    expect(ed.session.arm(1, -1)).toBe(false);
    expect(ed.session.active).toBe(false);
    expect(ed.edits.pausing).toBe(false);
  });

  it("step: moves from armed and moving, is ignored when idle and while settling", () => {
    const want: Record<DragState, [boolean, DragState, number[]]> = {
      idle: [false, "idle", X0],
      armed: [true, "moving", [0, 13, 20]],
      moving: [true, "moving", [0, 13, 20]],
      settling: [false, "settling", [0, 15, 20]],
    };
    for (const s of STATES) {
      const ed = inState(s);
      expect(ed.session.step(3), s).toBe(want[s][0]);
      expect(ed.session.state, s).toBe(want[s][1]);
      expect(xs(ed), s).toEqual(want[s][2]);
    }
  });

  it("step: one that changes nothing leaves a pressed drag armed", () => {
    const ed = inState("armed");
    expect(ed.session.step(0)).toBe(false);
    expect(ed.session.state).toBe("armed");
  });

  it("drop: armed ends with nothing written; moving commits one step; settling and idle only wait", async () => {
    for (const s of STATES) {
      const ed = inState(s);
      const before = ed.history.length;
      const done = ed.session.drop();
      if (s === "settling") {
        expect(ed.session.state).toBe("settling");
        ed.drops[0]();
      }
      await done;
      expect(ed.session.state, s).toBe("idle");
      expect(ed.edits.pausing, s).toBe(false);
      expect(ed.history.length - before, s).toBe(s === "moving" || s === "settling" ? 1 : 0);
      expect(xs(ed), s).toEqual(s === "moving" || s === "settling" ? [0, 15, 20] : X0);
    }
  });

  it("drop: with the core, the session stays settling (pause held) until the answer is committed", async () => {
    const ed = inState("moving");
    ed.slow = true;
    const done = ed.session.drop();
    expect(ed.session.state).toBe("settling");
    expect(ed.edits.pausing).toBe(true);
    expect(ed.history.length).toBe(1);
    ed.drops[0]();
    await done;
    expect(ed.states).toEqual(["armed", "moving", "settling", "idle"]);
    expect(ed.history.length).toBe(2);
    expect(ed.edits.pausing).toBe(false);
  });

  it("drop: one that throws (now or from the core) still ends the session and its pause", async () => {
    const ed = inState("moving");
    ed.failDrop = true;
    expect(() => ed.session.drop()).toThrow("drop failed");
    expect(ed.session.state).toBe("idle");
    expect(ed.edits.pausing).toBe(false);

    const late = new Ed();
    const session = new DragSession<number, number>({
      pause: () => late.edits.pause(),
      hold: (i) => late.edits.hold("dormers", i),
      snapshot: () => "",
      step: () => true,
      drop: () => Promise.reject(new Error("core refused")),
      restore: () => {},
    });
    session.arm(1, 0);
    session.step(1);
    await expect(session.drop()).rejects.toThrow("core refused");
    expect(session.state).toBe("idle");
    expect(late.edits.pausing).toBe(false);
  });

  it("cancel: armed and moving go back to the job of the press, nothing committed; settling and idle are left alone", () => {
    for (const s of STATES) {
      const ed = inState(s);
      const can = s === "armed" || s === "moving";
      expect(ed.session.cancel(), s).toBe(can);
      expect(ed.session.state, s).toBe(can ? "idle" : s);
      expect(ed.history.length, s).toBe(1);
      if (can) {
        expect(xs(ed), s).toEqual(X0);
        expect(ed.edits.pausing, s).toBe(false);
      }
    }
  });

  it("cut: ends a moving drag where it is, as the release would; later steps of that press do nothing", async () => {
    const ed = inState("moving");
    await ed.session.cut();
    expect(ed.session.state).toBe("idle");
    expect(xs(ed)).toEqual([0, 15, 20]);
    expect(ed.history.length).toBe(2);
    expect(ed.session.step(30)).toBe(false);
    await ed.session.drop();
    expect(xs(ed)).toEqual([0, 15, 20]);
    expect(ed.history.length).toBe(2);
  });

  it("foreign write: throws in every open state when strict, never when idle or for the drag's own write", () => {
    for (const s of STATES) {
      const ed = inState(s);
      if (s === "idle") expect(() => ed.foreignCommit("x")).not.toThrow();
      else expect(() => ed.foreignCommit("x"), s).toThrow(/foreign commit while/);
      expect(() => ed.session.write(() => ed.session.foreign())).not.toThrow();
    }
  });

  it("foreign write, not strict: a held drag ends where the job is now and never rebuilds from before that write", async () => {
    for (const s of ["armed", "moving"] as const) {
      const ed = inState(s, false);
      ed.foreignCommit("kept");
      expect(ed.session.state, s).toBe("idle");
      expect(ed.edits.pausing, s).toBe(false);
      expect(ed.session.step(9), s).toBe(false);
      expect(ed.session.cancel(), s).toBe(false);
      await ed.session.drop();
      expect(ed.job.notes, s).toBe("kept");
      expect(xs(ed), s).toEqual(s === "moving" ? [0, 15, 20] : X0);
    }
    // A drop that settles is left to finish; its own check gives way.
    const ed = inState("settling", false);
    ed.foreignCommit("kept");
    expect(ed.session.state).toBe("settling");
  });

  it("defer: runs at once when idle; while a drag is open it runs once at the end, however often it was asked", async () => {
    for (const s of STATES) {
      const ed = inState(s);
      ed.outsideSave();
      ed.outsideSave();
      expect(ed.reloads, s).toBe(s === "idle" ? 2 : 0);
      if (s === "settling") ed.drops[0]();
      await ed.session.drop();
      await flush();
      expect(ed.reloads, s).toBe(s === "idle" ? 2 : 1);
      expect(ed.reloads, s).toBe(ed.reloadsDue);
    }
  });

  it("settled: resolves at once when idle, else only when the drag is over (also after a cancel)", async () => {
    for (const s of STATES) {
      const ed = inState(s);
      let over = false;
      void ed.session.settled().then(() => (over = true));
      await flush();
      expect(over, s).toBe(s === "idle");
      if (s === "settling") ed.drops[0]();
      else ed.session.cancel();
      await flush();
      expect(over, s).toBe(true);
    }
  });

  it("the target is held, not its index: an item removed before the first step ends the drag and the neighbour stays", () => {
    const ed = new Ed(false);
    ed.session.arm(1, 1);
    // What a missed guard would do: the pressed dormer is deleted (the user's own step).
    const before = JSON.parse(JSON.stringify(ed.job)) as Project;
    ed.job.dormers.splice(1, 1);
    ed.edits.own(() => before, ed.job as unknown as Project);
    expect(ed.session.step(5)).toBe(false);
    expect(ed.session.state).toBe("idle");
    expect(ed.steps).toBe(0);
    expect(ed.job.dormers).toEqual([
      { id: 0, x: 0 },
      { id: 2, x: 20 },
    ]);
    expect(ed.edits.pausing).toBe(false);
  });

  it("the target is held: an item that moved to another index (one added before it) ends the drag too", async () => {
    const ed = inState("moving", false);
    const before = JSON.parse(JSON.stringify(ed.job)) as Project;
    ed.job.dormers.unshift({ id: 9, x: -10 });
    ed.edits.own(() => before, ed.job as unknown as Project);
    const steps = ed.steps;
    expect(ed.session.step(7)).toBe(false);
    await ed.session.drop();
    expect(ed.steps).toBe(steps);
    expect(ed.session.state).toBe("idle");
    expect(xs(ed)).toEqual([-10, 0, 15, 20]);
  });

  it("an answer of the core that comes during the drag is applied only after it, on the dragged job", async () => {
    const ed = new Ed();
    const answer = ed.coreEdit(0);
    await flush();
    ed.session.arm(1, 1);
    ed.session.step(5);
    answer();
    await flush();
    expect(xs(ed)).toEqual([0, 15, 20]);
    await ed.session.drop();
    // Asked again on the dragged job (jobedits rule 5), then applied.
    answer();
    await ed.edits.idle();
    expect(xs(ed)).toEqual([1, 15, 20]);
    expect(ed.history.length).toBe(3);
  });

  it("Undo pressed while the drop settles undoes exactly the drop", async () => {
    const ed = inState("settling");
    expect(ed.undo()).toBe(true);
    await flush();
    expect(ed.undone).toBe(0);
    ed.drops[0]();
    await flush();
    expect(ed.undone).toBe(1);
    expect(xs(ed)).toEqual(X0);
    expect(ed.history.length).toBe(1);
  });
});

/** Deterministic random numbers (mulberry32). */
function rng(seed: number) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

describe("DragSession: random sequences", () => {
  it("whatever meets a drag, it ends idle with no pause left, never rebuilt from an old base, the reload run once", async () => {
    for (let seed = 1; seed <= 300; seed++) {
      const rand = rng(seed);
      const pick = (n: number) => Math.floor(rand() * n);
      const ed = new Ed(false);
      const answers: (() => void)[] = [];
      let undoQueued = 0;
      const log: string[] = [];
      for (let k = 0; k < 40; k++) {
        const ev = pick(11);
        log.push(`${ev}@${ed.session.state}`);
        const was = ed.session.state;
        const before = JSON.stringify(ed.job);
        if (ev === 0) {
          const ok = ed.session.arm(pick(3), pick(ed.job.dormers.length));
          expect(ok, `seed ${seed} ${log}`).toBe(was === "idle");
        } else if (ev === 1 || ev === 2) {
          const moved = ed.session.step(pick(7) - 3);
          if (was === "idle" || was === "settling") expect(moved).toBe(false);
          if (!moved && was !== "armed" && was !== "moving") expect(JSON.stringify(ed.job)).toBe(before);
        } else if (ev === 3) {
          ed.slow = rand() < 0.5;
          void ed.session.drop();
        } else if (ev === 4) {
          const did = ed.session.cancel();
          expect(did).toBe(was === "armed" || was === "moving");
          if (did) expect(JSON.stringify(ed.job), `seed ${seed} ${log}`).toBe(ed.history[ed.history.length - 1]);
        } else if (ev === 5) {
          void ed.session.cut();
        } else if (ev === 6) {
          // A foreign commit (a missed guard): ends a held drag where the job is.
          ed.foreignCommit("n");
          if (was === "armed" || was === "moving") expect(ed.session.state).toBe("idle");
        } else if (ev === 7) {
          ed.outsideSave();
        } else if (ev === 8) {
          if (ed.undo()) undoQueued++;
          else expect(was === "armed" || was === "moving").toBe(true);
        } else if (ev === 9) {
          answers.push(ed.coreEdit(pick(ed.job.dormers.length)));
        } else {
          // The core answers: a held drop, and every edit that waits.
          ed.drops.splice(0).forEach((r) => r());
          answers.splice(0).forEach((a) => a());
        }
        await flush();
        // Never a pause without a drag, never a drag without its pause.
        expect(ed.edits.pausing, `seed ${seed} ${log}`).toBe(ed.session.active);
      }
      // Everything lets go.
      void ed.session.drop();
      for (let i = 0; i < 6; i++) {
        ed.drops.splice(0).forEach((r) => r());
        answers.splice(0).forEach((a) => a());
        await flush();
      }
      await ed.edits.idle();
      await flush();
      expect(ed.session.state, `seed ${seed} ${log}`).toBe("idle");
      expect(ed.edits.pausing, `seed ${seed}`).toBe(false);
      expect(ed.edits.size, `seed ${seed}`).toBe(0);
      expect(ed.reloads, `seed ${seed} ${log}`).toBe(ed.reloadsDue);
      expect(ed.undone, `seed ${seed} ${log}`).toBe(undoQueued);
      expect(JSON.stringify(ed.job), `seed ${seed} ${log}`).toBe(ed.history[ed.history.length - 1]);
      expect(ed.states[ed.states.length - 1] ?? "idle").toBe("idle");
    }
  });
});
