// Audit 2026-11-23 TEST-1505 (ARCH-1504): the app's list of the core's notices (results.ts NOTICES, used by
// hasRoofWarning for the Results tab mark) is the core's WarningKind::is_notice, kind by kind.
import { describe, expect, it } from "vitest";
import { hasRoofWarning } from "./results";
import type { CalcResult, RoofWarning } from "./types";

// The module names are built at run time: the UI has no Node type definitions.
const fs: { readFileSync(p: URL, enc: "utf8"): string } = await import(/* @vite-ignore */ ["node", "fs"].join(":"));
const check = fs.readFileSync(new URL("../../../crates/tectora-core/src/check.rs", import.meta.url), "utf8");
const snake = (s: string) => s.replace(/[A-Z]/g, (c, i) => (i ? "_" : "") + c.toLowerCase());
const body = (re: RegExp) => (check.match(re)?.[1] ?? "");
const kinds = [...body(/pub enum WarningKind \{([\s\S]*?)\n\}/).matchAll(/^\s{4}([A-Z]\w*),?\s*$/gm)].map((m) => snake(m[1]));
const notices = [...body(/fn is_notice\(self\) -> bool \{([\s\S]*?)\n    \}/).matchAll(/WarningKind::(\w+)/g)].map((m) => snake(m[1]));
const result = (kind: string) => ({ parts: [{ geometry: { warnings: [{ kind, value: 1 } as unknown as RoofWarning] } }] }) as unknown as CalcResult;

describe("the Results tab mark follows the core's notices (TEST-1505)", () => {
  it("reads the core's kinds and notices", () => {
    expect(kinds.length).toBeGreaterThan(10);
    expect(notices.length).toBeGreaterThan(2);
  });
  it("marks every warning kind that is not a notice, and no notice", () => {
    for (const k of kinds) expect(hasRoofWarning(result(k)), k).toBe(!notices.includes(k));
  });
});
