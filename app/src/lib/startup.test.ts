import { describe, expect, it } from "vitest";
import { afterListening, startOpening } from "./startup";

// Round-6 audit (errors.md To check 75): a file opened at cold start whose
// import finished between taking the pending items and registering the
// listeners was neither taken nor heard. The pending items are taken only
// once both listeners are registered.
describe("afterListening", () => {
  it("takes the pending items only after every listener is registered", async () => {
    const order: string[] = [];
    let ready!: () => void;
    const slow = new Promise<void>((r) => (ready = () => { order.push("listening"); r(); }));
    const done = afterListening([Promise.resolve(), slow], () => order.push("take"));
    await Promise.resolve();
    await Promise.resolve();
    expect(order).toEqual([]);
    ready();
    await done;
    expect(order).toEqual(["listening", "take"]);
  });
  it("still takes them when a listener cannot be registered", async () => {
    let taken = 0;
    await afterListening([Promise.reject(new Error("no events")), Promise.resolve()], () => taken++);
    expect(taken).toBe(1);
  });
});

// TEST-604: the scan of App.svelte only looked for a take *before* the
// `afterListening(` call, so a take placed right after it (before the
// listeners resolved) passed. The wiring is `startOpening` now, run here
// with listeners that register late.
describe("startOpening", () => {
  /** Fakes of the Tauri side: registration resolves when `listen()` is called. */
  function fakes() {
    const order: string[] = [];
    const listen: (() => void)[] = [];
    const heard: { open?: (id: string) => void; failed?: () => void } = {};
    let pending: string | null = "job-1";
    let errors: string[] = ["bad-file"];
    const late = (name: string) =>
      new Promise<() => void>((r) =>
        listen.push(() => {
          order.push(`listening:${name}`);
          r(() => order.push(`removed:${name}`));
        }),
      );
    const deps = {
      onOpenProject: (cb: (id: string) => void) => ((heard.open = cb), late("open")),
      onOpenFailed: (cb: () => void) => ((heard.failed = cb), late("failed")),
      takePendingOpen: async () => {
        order.push("take:pending");
        const id = pending;
        pending = null;
        return id;
      },
      takeOpenErrors: async () => {
        order.push("take:errors");
        return errors.splice(0);
      },
      open: (id: string) => order.push(`open:${id}`),
      failed: (e: string) => order.push(`failed:${e}`),
    };
    return { order, listen, heard, deps, more: (e: string) => errors.push(e) };
  }
  const settle = async () => {
    for (let i = 0; i < 10; i++) await Promise.resolve();
  };

  it("takes nothing until both listeners are registered, then the pending job and the failures once", async () => {
    const f = fakes();
    const opening = startOpening(f.deps);
    await settle();
    expect(f.order).toEqual([]);
    f.listen[0]();
    await settle();
    // One listener is not enough: an event of the other kind would be lost.
    expect(f.order).toEqual(["listening:open"]);
    f.listen[1]();
    await opening.ready;
    await settle();
    expect(f.order.slice(0, 2)).toEqual(["listening:open", "listening:failed"]);
    expect(f.order.filter((o) => o.startsWith("take:")).sort()).toEqual(["take:errors", "take:pending"]);
    expect(f.order.indexOf("take:pending")).toBeGreaterThan(f.order.indexOf("listening:failed"));
    expect(f.order.indexOf("take:errors")).toBeGreaterThan(f.order.indexOf("listening:failed"));
    expect(f.order).toContain("open:job-1");
    expect(f.order).toContain("failed:bad-file");
  });

  it("a failure heard later is taken and told; a job heard later opens", async () => {
    const f = fakes();
    const opening = startOpening(f.deps);
    f.listen.forEach((l) => l());
    await opening.ready;
    await settle();
    f.order.length = 0;
    f.more("second");
    f.heard.failed!();
    f.heard.open!("job-2");
    await settle();
    expect(f.order).toEqual(["take:errors", "open:job-2", "failed:second"]);
    opening.stop();
    await settle();
    expect(f.order.slice(-2).sort()).toEqual(["removed:failed", "removed:open"]);
  });

  it("is the only place that takes: App.svelte starts it and takes nothing itself", () => {
    const files = import.meta.glob("../App.svelte", { query: "?raw", import: "default", eager: true }) as Record<string, string>;
    const app = Object.values(files)[0];
    expect(app.match(/startOpening\(/g)?.length).toBe(1);
    // The takes are only handed over (arrow functions inside the call), never called by App itself.
    const call = app.slice(app.indexOf("startOpening("), app.indexOf("});", app.indexOf("startOpening(")));
    const rest = app.replace(call, "");
    expect(rest).not.toMatch(/takePendingOpen|takeOpenErrors|afterListening|onOpenProject\(|onOpenFailed\(/);
    expect(call.match(/takePendingOpen/g)?.length).toBe(2);
    expect(call.match(/takeOpenErrors/g)?.length).toBe(2);
    expect(call).toMatch(/takePendingOpen: \(\) => api\.takePendingOpen\(\),/);
    expect(call).toMatch(/takeOpenErrors: \(\) => api\.takeOpenErrors\(\),/);
  });
});
