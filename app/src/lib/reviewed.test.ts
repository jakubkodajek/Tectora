// TEST-1404: the AI changes a user accepted in an older Tectora (kept in
// localStorage per job id) are read through own keys only: a job id that is
// an Object.prototype name reads 0, not the inherited member (ARCH-1301).
import { describe, expect, it, vi } from "vitest";

vi.hoisted(() => {
  const stored: Record<string, string> = { "tectora.reviewed": JSON.stringify({ dum: 5, constructor: 7 }) };
  const storage = {
    getItem: (k: string) => (Object.hasOwn(stored, k) ? stored[k] : null),
    setItem: (k: string, v: string) => void (stored[k] = v),
    removeItem: (k: string) => void delete stored[k],
  };
  Object.defineProperty(globalThis, "localStorage", { value: storage, configurable: true, writable: true });
});
vi.mock("./api", () => ({ api: {} }));
vi.mock("./theme.svelte", () => ({ applyTheme: () => {} }));

import * as settings from "./settings.svelte";

describe("reviewedAt (TEST-1404, ARCH-1301)", () => {
  it("reads a stored accept by its job id, and 0 for any other id, prototype names too", () => {
    expect(settings.reviewedAt("dum")).toBe(5);
    expect(settings.reviewedAt("constructor")).toBe(7);
    for (const id of ["toString", "valueOf", "hasOwnProperty", "__proto__", "isPrototypeOf", "nope"]) expect(settings.reviewedAt(id), id).toBe(0);
  });

  it("has no writer left: accepts are the core's review now", () => {
    expect("markReviewed" in settings).toBe(false);
  });
});
