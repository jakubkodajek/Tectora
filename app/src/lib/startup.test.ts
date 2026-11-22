import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { afterListening } from "./startup";

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
  it("is how App.svelte takes the opened files", () => {
    const app = readFileSync(new URL("../App.svelte", import.meta.url), "utf8");
    const at = app.indexOf("afterListening([unOpen, unOpenFailed]");
    expect(at).toBeGreaterThan(0);
    // No take before the listeners exist.
    expect(app.slice(0, at)).not.toMatch(/takePendingOpen\(|showOpenErrors\(\)/);
  });
});
