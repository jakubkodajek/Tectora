import { describe, expect, it } from "vitest";
import { buildDormer, defaultParams, parseDormer } from "./dormers";
import { MAX_ROUND_SEGMENTS, arcFor } from "./roundroof";

describe("round segment cap (MATH-018)", () => {
  it("refuses more segments than the core allows", () => {
    const a = { span: 2, overhang: 0, eave_height: 4, mode: "default" as const, value: 0 };
    expect(arcFor({ ...a, segments: MAX_ROUND_SEGMENTS })).not.toBeNull();
    expect(arcFor({ ...a, segments: MAX_ROUND_SEGMENTS + 1 })).toBeNull();
  });

  it("builds a round dormer with too many segments like the core: round, at the cap", () => {
    const p = { ...defaultParams("round", { x: 0, y: 0 }, 0, 4), width: 2, segments: 40 };
    const d = buildDormer(p);
    expect(parseDormer(d).kind).toBe("round");
    expect((d.edges[0].breaks?.length ?? 0) + 1).toBe(MAX_ROUND_SEGMENTS);
  });
});
