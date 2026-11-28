// The review of an outside tower edit lists what changed (audit UX-008).
import { describe, expect, it } from "vitest";
import { i18n } from "./i18n.svelte";
import { towerDetails } from "./changes";
import type { Change, Tower } from "./types";

const base: Tower = { center: { x: 4, y: 2 }, width: 3, base_height: 9.78, tiers: [{ rise: 2, top_width: 1 }, { rise: 0.9, top_width: 0 }] };

describe("tower change details", () => {
  it("names each changed field and tier with before and after", () => {
    i18n.locale = "en";
    const after: Tower = { ...base, base_height: 9, tiers: [base.tiers[0], { rise: 1.4, top_width: 0 }] };
    const c = { field: "tower", item: 0, kind: "modified", before: base, after } as unknown as Change;
    const d = towerDetails(c, null);
    expect(d.some((x) => x.includes("9.78 m → 9.00 m"))).toBe(true);
    expect(d.some((x) => x === "Tier 2, Rise: 0.90 m → 1.40 m")).toBe(true);
    expect(d).toHaveLength(2);
  });

  it("nothing for other changes or values of the wrong type", () => {
    expect(towerDetails({ field: "tower", item: 0, kind: "added", after: base } as unknown as Change, null)).toEqual([]);
    expect(towerDetails({ field: "tower", item: 0, kind: "modified", before: "x", after: 3 } as unknown as Change, null)).toEqual([]);
    expect(towerDetails({ field: "pitch_deg", edge: 0, before: 30, after: 40 } as unknown as Change, null)).toEqual([]);
  });
});
