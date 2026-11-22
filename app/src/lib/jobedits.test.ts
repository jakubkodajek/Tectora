import { describe, expect, it } from "vitest";
import { t } from "./i18n.svelte";
import { JobEdits, TRIES, follow, notMade } from "./jobedits";
import type { Project } from "./types";

const dormer = (x: number, w = 1.5) => ({ edges: [{ start: { x, y: 1.6 } }], end: { x: x + w, y: 1.6 } });
const part = (name: string, pitch = 35) => ({ name, edges: [{ start: { x: 0, y: 0 }, pitch_deg: pitch, eave_height: 3, overhang: 0.5 }] });
const job = (over: Record<string, unknown> = {}): Project =>
  ({ name: "J", notes: "", job: { status: "draft" }, parts: [part("Main")], dormers: [dormer(1.5), dormer(5), dormer(8.5)], chimneys: [], ...over }) as unknown as Project;
/** The same job as the editor holds it after an edit: other key order, explicit defaults. */
const editorForm = (p: Project): Project => {
  const flip = (v: unknown): unknown =>
    Array.isArray(v) ? v.map(flip) : v && typeof v === "object" ? Object.fromEntries([...Object.entries(v as object).reverse().map(([k, x]) => [k, flip(x)]), ["half_hip", null]]) : v;
  return flip(p) as Project;
};
const clone = <T>(v: T): T => JSON.parse(JSON.stringify(v));

function editor(start: Project) {
  const state = { job: start as Project | null };
  const refused: string[] = [];
  const edits = new JobEdits(
    () => state.job,
    () => JSON.stringify(state.job),
    (o) => refused.push(o),
  );
  return { state, edits, refused };
}

/** A core call that answers when `release()` is called. */
function slowCore() {
  const waiting: (() => void)[] = [];
  return {
    wait: () => new Promise<void>((r) => waiting.push(r)),
    release: () => waiting.shift()?.(),
    get asked() {
      return waiting.length;
    },
  };
}
const tick = () => new Promise((r) => setTimeout(r, 0));

describe("follow", () => {
  it("keeps the index of an unchanged item", () => {
    expect(follow([1, 2, 3], [1, 2, 3], 1)).toBe(1);
    expect(follow([1, 2, 3], [9, 2, 3], 1)).toBe(1);
  });
  it("follows an item past one removed or added before it", () => {
    expect(follow(["a", "b", "c"], ["b", "c"], 1)).toBe(0);
    expect(follow(["a", "b", "c"], ["b", "c"], 2)).toBe(1);
    expect(follow(["a", "b"], ["x", "a", "b"], 1)).toBe(2);
  });
  it("is null for an item changed or removed", () => {
    expect(follow(["a", "b", "c"], ["b", "c"], 0)).toBeNull();
    expect(follow(["a", "b"], ["a", "B"], 1)).toBeNull();
    expect(follow(["a"], undefined, 0)).toBeNull();
    expect(follow(["a"], ["a"], 3)).toBeNull();
  });
  it("tells equal items apart by their order, or not at all", () => {
    expect(follow(["w", "x", "w"], ["x", "w", "w"], 2)).toBe(2);
    expect(follow(["w", "x", "w"], ["x", "w", "w"], 0)).toBe(1);
    expect(follow(["w", "x", "w"], ["x", "w"], 2)).toBeNull();
  });
  it("does not guess among equal items when their number changed: one twin gone and an item added before the other (TEST-606 J6)", () => {
    // The list kept its length, the held twin's place holds something else,
    // and one twin is left: it may be the other one, so the item is let go.
    expect(follow(["w", "w"], ["n", "w"], 0)).toBeNull();
    expect(follow(["w", "x", "w"], ["n", "x", "w"], 0)).toBeNull();
  });
});

describe("notMade", () => {
  it("says why per outcome, and that an item the user removed himself was removed (TEST-606 K10)", () => {
    expect(notMade("done")).toBeUndefined();
    expect(notMade("gone")).toBe(t("editor.actionStale"));
    expect(notMade("restless")).toBe(t("editor.actionBusy"));
    expect(notMade("removed")).toBe(t("editor.actionRemoved"));
    expect(notMade("closed")).toBe(t("num.refused"));
    // Four different texts: "removed" is not the generic refusal.
    expect(new Set([notMade("gone"), notMade("restless"), notMade("removed"), notMade("closed")]).size).toBe(4);
  });
});

describe("holds across an outside save", () => {
  it("follows the dormer when the AI deletes an earlier one (ERR-302)", () => {
    const file = job();
    const { state, edits } = editor(editorForm(file));
    const h = edits.hold("dormers", 1);
    const remote = job({ dormers: [dormer(5), dormer(8.5)] });
    const before = state.job!;
    state.job = clone(remote);
    edits.outside(file, remote, before, state.job);
    expect(h).toMatchObject({ index: 0, gone: false });
  });

  it("is gone when the AI deletes or changes the held item itself (ERR-302)", () => {
    const file = job();
    const { state, edits } = editor(clone(file));
    const deleted = edits.hold("dormers", 0);
    const changed = edits.hold("dormers", 2);
    const remote = job({ dormers: [dormer(5), dormer(8.5, 2)] });
    const before = state.job!;
    state.job = clone(remote);
    edits.outside(file, remote, before, state.job);
    expect(deleted.gone).toBe(true);
    expect(changed.gone).toBe(true);
  });

  it("stays when the save changed only other things, whatever form the editor's copy has (ERR-301)", () => {
    const file = job();
    const { state, edits } = editor(editorForm(file));
    const h = edits.hold("parts", 0);
    const d = edits.hold("dormers", 2);
    const remote = job({ notes: "AI note" });
    const before = state.job!;
    state.job = clone(remote);
    edits.outside(file, remote, before, state.job);
    expect(h).toMatchObject({ index: 0, gone: false });
    expect(d).toMatchObject({ index: 2, gone: false });
  });

  it("stays on an item only the user's unsaved edit changed", () => {
    const file = job();
    const mine = clone(file);
    (mine.dormers[1] as any).end.x = 7;
    const { state, edits } = editor(mine);
    const h = edits.hold("dormers", 1);
    const remote = job({ notes: "AI note" });
    state.job = { ...clone(mine), notes: "AI note" };
    edits.outside(file, remote, mine, state.job);
    expect(h.gone).toBe(false);
  });

  it("gives up when an unsaved edit of the user changed the length of a list the save changed too", () => {
    const file = job();
    const mine = clone(file);
    mine.dormers.push(dormer(11) as any);
    const { state, edits } = editor(mine);
    const h = edits.hold("dormers", 1);
    const remote = job({ dormers: [dormer(5), dormer(8.5)] });
    state.job = clone(remote);
    edits.outside(file, remote, mine, state.job);
    expect(h.gone).toBe(true);
  });

  it("a loose hold (the selection) stays on an item changed in place and goes with a deleted one", () => {
    const file = job();
    const { state, edits } = editor(clone(file));
    const sel = edits.hold("dormers", 1, true);
    const remote = job({ dormers: [dormer(1.5), dormer(5, 2), dormer(8.5)] });
    let before = state.job!;
    state.job = clone(remote);
    edits.outside(file, remote, before, state.job);
    expect(sel).toMatchObject({ index: 1, gone: false });
    const remote2 = job({ dormers: [dormer(1.5), dormer(8.5)] });
    before = state.job!;
    state.job = clone(remote2);
    edits.outside(remote, remote2, before, state.job);
    expect(sel.gone).toBe(true);
  });

  it("a hold on nothing is gone at once", () => {
    const { edits } = editor(job());
    expect(edits.hold("dormers", 7).gone).toBe(true);
    expect(edits.hold("windows", 0).gone).toBe(true);
  });
});

describe("holds across the user's own edits", () => {
  it("follows the item when the user deletes an earlier one, and stays through an own change of it", () => {
    const { state, edits } = editor(job());
    const h = edits.hold("dormers", 2);
    let before = clone(state.job!);
    (state.job!.dormers[2] as any).end.x = 11;
    edits.own(() => before, state.job!);
    expect(h).toMatchObject({ index: 2, gone: false });
    before = clone(state.job!);
    state.job!.dormers.splice(0, 1);
    edits.own(() => before, state.job!);
    expect(h).toMatchObject({ index: 1, gone: false });
    before = clone(state.job!);
    state.job!.dormers.splice(1, 1);
    edits.own(() => before, state.job!);
    expect(h.gone).toBe(true);
  });

  it("does not read the earlier job while nothing is held", () => {
    const { state, edits } = editor(job());
    let read = 0;
    edits.own(() => (read++, state.job!), state.job!);
    expect(read).toBe(0);
  });
});

describe("the roof hold", () => {
  it("survives an outside save of the notes after an own edit left the editor's copy in another form (ERR-301)", () => {
    const file = job();
    const { state, edits } = editor(editorForm(file));
    const h = edits.holdRoof();
    const remote = job({ notes: "AI note" });
    const before = state.job!;
    state.job = clone(remote);
    edits.outside(file, remote, before, state.job);
    expect(edits.roofSame(h)).toBe(true);
  });

  it("is changed by an outside save of the roof", () => {
    const file = job();
    const { state, edits } = editor(clone(file));
    const h = edits.holdRoof();
    const remote = job({ parts: [part("Main", 30)] });
    const before = state.job!;
    state.job = clone(remote);
    edits.outside(file, remote, before, state.job);
    expect(edits.roofSame(h)).toBe(false);
  });

  it("is changed by the user's own edit of the roof, also when an outside save of the notes follows (ERR-201)", () => {
    const file = job();
    const { state, edits } = editor(clone(file));
    const h = edits.holdRoof();
    const h2 = edits.holdRoof();
    (state.job!.parts[0].edges[0] as any).pitch_deg = 40;
    expect(edits.roofSame(h)).toBe(false);
    const remote = job({ notes: "AI note" });
    const before = state.job!;
    state.job = { ...clone(before), notes: "AI note" };
    edits.outside(file, remote, before, state.job);
    expect(edits.roofSame(h2)).toBe(false);
  });

  it("ignores the job's texts and status", () => {
    const { state, edits } = editor(job());
    const h = edits.holdRoof();
    state.job!.notes = "typed";
    (state.job!.job as any).status = "offer";
    expect(edits.roofSame(h)).toBe(true);
  });
});

describe("run", () => {
  const width = (p: Project, i: number) => (p.dormers[i] as any).end.x - (p.dormers[i] as any).edges[0].start.x;
  const setWidth = (edits: JobEdits, state: { job: Project | null }, at: number, w: number, core: ReturnType<typeof slowCore>) => {
    const h = edits.hold("dormers", at);
    return edits.run(
      h,
      async (p, i) => {
        const x = (p.dormers[i] as any).edges[0].start.x;
        await core.wait();
        return x + w;
      },
      (end, i) => ((state.job!.dormers[i] as any).end.x = end),
    );
  };

  it("applies quick steps one after another, each on what the one before left (UI-301)", async () => {
    const { state, edits, refused } = editor(job());
    const core = slowCore();
    const h = edits.hold("dormers", 0);
    const step = () =>
      edits.run(
        h,
        async (p, i) => {
          const w = width(p, i);
          await core.wait();
          return +(w + 0.05).toFixed(2);
        },
        (w, i) => ((state.job!.dormers[i] as any).end.x = (state.job!.dormers[i] as any).edges[0].start.x + w),
      );
    const all = [step(), step(), step(), step()];
    expect(edits.size).toBe(4);
    for (let i = 0; i < 4; i++) {
      await tick();
      expect(core.asked).toBe(1);
      core.release();
    }
    expect(await Promise.all(all)).toEqual(["done", "done", "done", "done"]);
    expect(width(state.job!, 0)).toBeCloseTo(1.7);
    expect(refused).toEqual([]);
    expect(edits.size).toBe(0);
  });

  it("lands on the dormer it was typed for after the AI deletes an earlier one meanwhile (ERR-302)", async () => {
    const file = job();
    const { state, edits, refused } = editor(clone(file));
    const core = slowCore();
    const done = setWidth(edits, state, 1, 2, core);
    await tick();
    const remote = job({ dormers: [dormer(5), dormer(8.5)] });
    const before = state.job!;
    state.job = clone(remote);
    edits.outside(file, remote, before, state.job);
    core.release();
    await tick();
    core.release();
    expect(await done).toBe("done");
    expect(state.job!.dormers.map((_, i) => width(state.job!, i))).toEqual([2, 1.5]);
    expect(refused).toEqual([]);
  });

  it("is refused, once, when the AI deletes the very dormer meanwhile (ERR-302)", async () => {
    const file = job();
    const { state, edits, refused } = editor(clone(file));
    const core = slowCore();
    const done = setWidth(edits, state, 0, 2, core);
    await tick();
    const remote = job({ dormers: [dormer(5), dormer(8.5)] });
    const before = state.job!;
    state.job = clone(remote);
    edits.outside(file, remote, before, state.job);
    core.release();
    expect(await done).toBe("gone");
    expect(state.job!.dormers.map((_, i) => width(state.job!, i))).toEqual([1.5, 1.5]);
    expect(refused).toEqual(["gone"]);
  });

  it("asks again when the user's own direct edit changed the job meanwhile, and keeps both (UI-301)", async () => {
    const { state, edits, refused } = editor(job());
    const core = slowCore();
    let asks = 0;
    const h = edits.hold("parts", 0);
    const done = edits.run(
      h,
      async (p, i) => {
        asks++;
        const copy = clone(p.parts[i]);
        await core.wait();
        return { ...copy, name: "Longer" };
      },
      (next, i) => (state.job!.parts[i] = next as any),
    );
    await tick();
    (state.job!.parts[0].edges[0] as any).pitch_deg = 37;
    core.release();
    await tick();
    core.release();
    expect(await done).toBe("done");
    expect(asks).toBe(2);
    expect(state.job!.parts[0]).toMatchObject({ name: "Longer", edges: [{ pitch_deg: 37 }] });
    expect(refused).toEqual([]);
  });

  it("gives up with a note when the job never holds still", async () => {
    const { state, edits, refused } = editor(job());
    let asks = 0;
    const done = edits.run(
      edits.hold("parts", 0),
      async () => {
        asks++;
        state.job!.notes = `n${asks}`;
      },
      () => {
        throw new Error("must not be applied");
      },
    );
    expect(await done).toBe("restless");
    expect(asks).toBe(TRIES);
    expect(refused).toEqual(["restless"]);
  });

  it("hands a refusal of the core to the caller and goes on with the next edit", async () => {
    const { state, edits } = editor(job());
    const h = edits.hold("parts", 0);
    const bad = edits.run(h, async () => Promise.reject({ code: "refused" }), () => {});
    const good = edits.run(h, async () => "Named", (n, i) => (state.job!.parts[i].name = n));
    await expect(bad).rejects.toEqual({ code: "refused" });
    expect(await good).toBe("done");
    expect(state.job!.parts[0].name).toBe("Named");
    expect(edits.size).toBe(0);
  });

  it("ends quietly when the editor closed", async () => {
    const { state, edits, refused } = editor(job());
    const core = slowCore();
    const done = setWidth(edits, state, 0, 2, core);
    await tick();
    state.job = null;
    core.release();
    expect(await done).toBe("closed");
    expect(refused).toEqual([]);
  });
});

// Round-5 audit (TEST-401, UX-402, ERR-401).
describe("round 5: the rule's other branches", () => {
  it("a strict hold on an item the AI changed in place is gone, a loose one stays (TEST-401, anchor case 4)", () => {
    const file = job();
    const { state, edits } = editor(clone(file));
    const strict = edits.hold("dormers", 1);
    const loose = edits.hold("dormers", 1, true);
    const remote = job({ dormers: [dormer(1.5), dormer(5, 2), dormer(8.5)] });
    const before = state.job!;
    state.job = clone(remote);
    edits.outside(file, remote, before, state.job);
    expect(strict.gone).toBe(true);
    expect(loose).toMatchObject({ index: 1, gone: false });
  });

  it("a hold stays when the user added an item unsaved and the AI saved only the notes (TEST-401, the merge on save)", () => {
    const file = job();
    const mine = clone(file);
    mine.dormers.push(dormer(11) as any);
    const { state, edits } = editor(mine);
    const h = edits.hold("dormers", 1);
    const remote = job({ notes: "AI" });
    state.job = { ...clone(mine), notes: "AI" };
    edits.outside(file, remote, mine, state.job);
    expect(h).toMatchObject({ index: 1, gone: false });
  });

  it("idle waits for edits queued while it waits (TEST-401)", async () => {
    const { state, edits } = editor(job());
    const core = slowCore();
    const applied: number[] = [];
    const h = edits.hold("parts", 0);
    const edit = (n: number) =>
      edits.run(
        h,
        async () => {
          await core.wait();
          return n;
        },
        (v) => {
          applied.push(v);
          state.job!.notes = `n${v}`;
        },
      );
    void edit(1);
    let idle = false;
    const waited = edits.idle().then(() => (idle = true));
    void edit(2);
    await tick();
    core.release();
    await tick();
    await tick();
    expect(applied).toEqual([1]);
    expect(idle).toBe(false);
    core.release();
    await waited;
    expect(applied).toEqual([1, 2]);
    expect(edits.size).toBe(0);
  });

  it("ends quietly when the user's own step deleted the held item meanwhile (UX-402)", async () => {
    const { state, edits, refused } = editor(job());
    const core = slowCore();
    const h = edits.hold("dormers", 1);
    const done = edits.run(
      h,
      async () => {
        await core.wait();
        return 2;
      },
      () => {
        throw new Error("must not be applied");
      },
    );
    await tick();
    const before = clone(state.job!);
    state.job!.dormers.splice(1, 1);
    edits.own(() => before, state.job!);
    core.release();
    expect(await done).toBe("removed");
    expect(refused).toEqual([]);
    expect(h).toMatchObject({ gone: true, removed: true });
  });

  it("an equal twin left behind is no removal: the edit is refused with a note", () => {
    const { state, edits } = editor(job({ dormers: [dormer(1.5), dormer(1.5), dormer(8.5)] }));
    const h = edits.hold("dormers", 1);
    const before = clone(state.job!);
    // One of two equal dormers deleted: which of them was held cannot be told.
    state.job!.dormers.splice(0, 1);
    edits.own(() => before, state.job!);
    expect(h.gone).toBe(true);
    expect(h.removed).toBe(false);
  });

  it("hears a refusal with the hold it was for, to name the object", async () => {
    const file = job();
    const heard: [string, unknown][] = [];
    const state = { job: clone(file) as Project | null };
    const edits = new JobEdits(
      () => state.job,
      () => JSON.stringify(state.job),
      (o, h) => heard.push([o, h && { list: h.list, index: h.index }]),
    );
    const core = slowCore();
    const done = edits.run(edits.hold("dormers", 0), async () => core.wait(), () => {});
    await tick();
    const remote = job({ dormers: [dormer(5), dormer(8.5)] });
    const before = state.job!;
    state.job = clone(remote);
    edits.outside(file, remote, before, state.job);
    core.release();
    expect(await done).toBe("gone");
    expect(heard).toEqual([["gone", { list: "dormers", index: 0 }]]);
  });

  it("applies no answer while a drag is under way, and asks again on the dragged job (ERR-401)", async () => {
    const { state, edits, refused } = editor(job());
    const core = slowCore();
    const seen: number[] = [];
    const done = edits.run(
      edits.hold("dormers", 1),
      async (p, i) => {
        const x = (p.dormers[2] as any).edges[0].start.x;
        seen.push(x);
        await core.wait();
        return { i, x };
      },
      ({ i }) => ((state.job!.dormers[i] as any).end.x = 7),
    );
    await tick();
    const end = edits.pause();
    expect(edits.pausing).toBe(true);
    // The answer comes while the pointer is held still: not applied.
    core.release();
    await tick();
    await tick();
    expect((state.job!.dormers[1] as any).end.x).toBe(6.5);
    // Every drag step rebuilds the job from its start, then the drag ends.
    const start = JSON.stringify(state.job);
    for (const dy of [1, 2, 3]) {
      state.job = JSON.parse(start);
      (state.job!.dormers[2] as any).edges[0].start.x = 8.5 + dy;
    }
    end();
    end();
    expect(edits.pausing).toBe(false);
    await tick();
    expect(core.asked).toBe(1);
    core.release();
    expect(await done).toBe("done");
    expect(seen).toEqual([8.5, 11.5]);
    expect((state.job!.dormers[1] as any).end.x).toBe(7);
    expect((state.job!.dormers[2] as any).edges[0].start.x).toBe(11.5);
    expect(refused).toEqual([]);
  });

  it("an edit queued during a drag waits for its end before it asks (ERR-401)", async () => {
    const { state, edits } = editor(job());
    let asked = 0;
    const end = edits.pause();
    const done = edits.run(
      edits.hold("parts", 0),
      async () => {
        asked++;
        return "Named";
      },
      (n, i) => (state.job!.parts[i].name = n),
    );
    await tick();
    await tick();
    expect(asked).toBe(0);
    end();
    expect(await done).toBe("done");
    expect(asked).toBe(1);
    expect(state.job!.parts[0].name).toBe("Named");
  });
});

describe("round 6: overlapping drag pauses (ERR-501, TEST-506 item 4)", () => {
  const queued = (state: { job: Project | null }, edits: JobEdits) => {
    const seen = { asked: 0 };
    const done = edits.run(
      edits.hold("parts", 0),
      async () => {
        seen.asked++;
        return "Named";
      },
      (n, i) => (state.job!.parts[i].name = n),
    );
    return { seen, done };
  };

  it("holds an edit until the last of two overlapping pauses ends, whichever ends first", async () => {
    for (const order of [[0, 1], [1, 0]]) {
      const { state, edits } = editor(job());
      const ends = [edits.pause(), edits.pause()];
      const { seen, done } = queued(state, edits);
      ends[order[0]]();
      await tick();
      await tick();
      expect(seen.asked).toBe(0);
      expect(edits.pausing).toBe(true);
      ends[order[1]]();
      expect(await done).toBe("done");
      expect(seen.asked).toBe(1);
      expect(edits.pausing).toBe(false);
    }
  });

  it("an end called twice counts once: the other pause still holds", async () => {
    const { state, edits } = editor(job());
    const first = edits.pause();
    const second = edits.pause();
    const { seen, done } = queued(state, edits);
    second();
    second();
    await tick();
    await tick();
    expect(seen.asked).toBe(0);
    expect(edits.pausing).toBe(true);
    first();
    expect(await done).toBe("done");
    // A late extra call after the end changes nothing.
    first();
    expect(edits.pausing).toBe(false);
    const again = queued(state, edits);
    expect(await again.done).toBe("done");
  });
});

describe("heldName (UX-401, UX-404)", () => {
  it("names the object a refused edit was for", async () => {
    const { heldName } = await import("./jobedits");
    const p = job({ parts: [part("Main"), part("Garáž")], chimneys: [{}, {}] });
    const { edits } = editor(p);
    expect(heldName(edits.hold("chimneys", 1), p)).toBe(t("opening.chimneyN", { n: 2 }));
    expect(heldName(edits.hold("dormers", 2), p)).toBe(t("dormer.n", { n: 3 }));
    expect(heldName(edits.hold("parts", 1), p)).toBe("Garáž");
    const gone = edits.hold("parts", 1);
    gone.gone = true;
    expect(heldName(gone, p)).toBe(t("prob.item.part", { n: 2 }));
  });
});
