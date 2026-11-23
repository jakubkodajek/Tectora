// Audit 2026-11-23 ARCH-901: the actor names only Tectora writes are one
// list in the core (store.rs `RESERVED_ACTORS`); the app's copies in
// types.ts must name the same, or an entry of a new reserved name would be
// shown and reviewed as an AI's (or an AI's as Tectora's).
import { describe, expect, it } from "vitest";
import { ENGINE_ACTOR, RECEIVED_ENGINE_ACTOR, USER_ACTOR, isAi } from "./types";

// The module names are built at run time: the UI has no Node type definitions.
const fs: { readFileSync(p: URL, enc: "utf8"): string } = await import(/* @vite-ignore */ ["node", "fs"].join(":"));
const store = fs.readFileSync(new URL("../../../crates/tectora-core/src/store.rs", import.meta.url), "utf8");

/** The value of `pub const <name>: &str = "...";` in store.rs. */
function rustStr(name: string): string {
  const m = new RegExp(`pub const ${name}: &str = "([^"]*)";`).exec(store);
  if (!m) throw new Error(`no ${name} in store.rs`);
  return m[1];
}

describe("reserved actors", () => {
  it("are the same in the core and the app", () => {
    const m = /pub const RESERVED_ACTORS: \[&str; \d+\] = \[([^\]]*)\];/.exec(store);
    expect(m, "RESERVED_ACTORS in store.rs").not.toBeNull();
    const reserved = m![1].split(",").map((s) => s.trim()).filter(Boolean).map(rustStr);
    expect(reserved.sort()).toEqual([USER_ACTOR, ENGINE_ACTOR, RECEIVED_ENGINE_ACTOR].sort());
    for (const r of reserved) expect(isAi(r), r).toBe(false);
    expect(isAi("claude")).toBe(true);
  });
});
