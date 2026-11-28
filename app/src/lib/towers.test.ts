import { describe, expect, it } from "vitest";
import { i18n } from "./i18n.svelte";
import { deleteItem, itemCentre, moveItem, rotateItem, transformItem } from "./items";
import { mirror, rotation } from "./transform";
import { defaultTowerBase, roughFootprint, towerSection } from "./towers";
import { areaRows, emptyRoof, isFacesOnly, towerRows } from "./results";
import { arcReason, refusalReason } from "./errors";
import { placeSteps } from "./placesteps";
import { hasBuilding } from "./offer";
import { emptyProject, type CalcResult, type Project, type Tower, type TowerResult } from "./types";

const tower = (over: Partial<Tower> = {}): Tower => ({
  label: "",
  center: { x: 4, y: 2 },
  rotation_deg: 30,
  width: 2,
  base_height: 9,
  tiers: [{ rise: 1.5, top_width: 0 }],
  ...over,
});

const withTower = (): Project => {
  const p = emptyProject("T");
  p.towers = [tower()];
  return p;
};

// A square pyramid tower 2 m wide, its roof from 9 m to 10.5 m (as the core gives it).
const result: TowerResult = {
  top_height: 10.5,
  base_perimeter: 8,
  base_area: 4,
  tiers: [
    {
      index: 0,
      kind: "roof",
      material: 2,
      pitch_deg: 56.31,
      area: 7.21,
      bottom_height: 9,
      top_height: 10.5,
      bottom_perimeter: 8,
      top_perimeter: 0,
      hip_count: 4,
      hip_length: 8.25,
      rings: [
        [[3, 1, 9], [5, 1, 9], [5, 3, 9], [3, 3, 9]],
        [[4, 2, 10.5], [4, 2, 10.5], [4, 2, 10.5], [4, 2, 10.5]],
      ],
    },
  ],
  lines: [],
};

describe("towers as plan items (as tectora-core map_tower)", () => {
  it("moves with its axis and keeps its turn", () => {
    const p = withTower();
    moveItem(p, { kind: "tower", index: 0 }, 1, -0.5);
    expect(p.towers![0].center).toEqual({ x: 5, y: 1.5 });
    expect(p.towers![0].rotation_deg).toBe(30);
  });

  it("turns about its own axis: the centre stays, the turn adds up", () => {
    const p = withTower();
    rotateItem(p, { kind: "tower", index: 0 }, 90);
    expect(p.towers![0].center.x).toBeCloseTo(4, 9);
    expect(p.towers![0].center.y).toBeCloseTo(2, 9);
    expect(p.towers![0].rotation_deg).toBe(120);
    rotateItem(p, { kind: "tower", index: 0 }, -150);
    expect(p.towers![0].rotation_deg).toBe(330);
  });

  it("is mirrored by its axis and its turn, and copies append", () => {
    const p = withTower();
    const out = transformItem(p, { kind: "tower", index: 0 }, mirror({ x: 0, y: 0 }, 0), 1);
    expect(out).toEqual([{ kind: "tower", index: 1 }]);
    expect(p.towers![1].center.x).toBeCloseTo(4, 9);
    expect(p.towers![1].center.y).toBeCloseTo(-2, 9);
    expect(p.towers![1].rotation_deg).toBe(330);
    transformItem(p, { kind: "tower", index: 1 }, rotation({ x: 0, y: 0 }, 90));
    expect(p.towers![1].center.x).toBeCloseTo(2, 9);
    expect(p.towers![1].center.y).toBeCloseTo(4, 9);
    expect(p.towers![1].rotation_deg).toBe(60);
  });

  it("has its axis as centre and is deleted", () => {
    const p = withTower();
    expect(itemCentre(p, { kind: "tower", index: 0 })).toEqual({ x: 4, y: 2 });
    deleteItem(p, { kind: "tower", index: 0 });
    expect(p.towers).toEqual([]);
    expect(itemCentre(p, { kind: "tower", index: 0 })).toBeNull();
  });
});

describe("tower drawing helpers", () => {
  it("the section has the half width at every ring and drops repeated rings", () => {
    const s = towerSection({ center: { x: 4, y: 2 }, rotation_deg: 0 }, result);
    expect(s).toEqual([{ half: 1, z: 9 }, { half: 0, z: 10.5 }]);
    expect(towerSection({ center: { x: 0, y: 0 } }, null)).toEqual([]);
  });

  it("a rough footprint of a turned rectangle, a polygon and a circle; none for no numbers", () => {
    const r = roughFootprint(tower({ rotation_deg: 90, depth: 1 }));
    expect(r).toHaveLength(4);
    expect(r[0].x).toBeCloseTo(4.5, 9);
    expect(r[0].y).toBeCloseTo(1, 9);
    expect(roughFootprint(tower({ shape: "polygon", sides: 8 }))).toHaveLength(8);
    expect(roughFootprint(tower({ shape: "circle" }))).toHaveLength(32);
    expect(roughFootprint(tower({ width: Number.NaN }))).toEqual([]);
    expect(roughFootprint(tower({ width: -1 }))).toEqual([]);
  });

  it("a new tower's roof starts 3 m above the highest roof under it, else above the eave", () => {
    const slope = (p: { x: number; y: number }) => (p.x >= 0 ? 5 + p.y * 0.5 : null);
    expect(defaultTowerBase(slope, { x: 1, y: 2 }, 3, 4)).toBe(5 + 3.5 * 0.5 + 3);
    expect(defaultTowerBase(() => null, { x: 1, y: 2 }, 3, 4)).toBe(7);
  });
});

describe("towers in Results", () => {
  const calc = (towers: CalcResult["towers"]): CalcResult => ({ parts: [], totals: {} as never, codes: {}, problems: [], towers });

  it("lists each calculated tower's roof in the areas, and every tower with its tiers", () => {
    i18n.locale = "cs";
    const r = calc([
      { label: "", result, roof_area: 7.21, wall_area: 0, shaft: { bottom: [], wall_area: 12.5, on_roof: false, base_below_roof: false } },
      { label: "Lucerna", result: null },
    ]);
    expect(areaRows(r)).toEqual([{ name: "Věž 1", area: 7.21, part: -1, manual: false, tower: true }]);
    const rows = towerRows(r);
    expect(rows[0]).toMatchObject({ name: "Věž 1", calculated: true, roof_area: 7.21, shaft_area: 12.5, top_height: 10.5, on_ground: true, cap: null });
    expect(rows[0].tiers).toEqual([{ n: 1, wall: false, pitch_deg: 56.31, area: 7.21, material: 2 }]);
    expect(rows[1]).toMatchObject({ name: "Lucerna", calculated: false, tiers: [] });
  });

  it("a job of towers only is not empty and has no building lines of its own", () => {
    const p = withTower();
    expect(hasBuilding(p)).toBe(true);
    expect(isFacesOnly(p)).toBe(true);
    expect(emptyRoof(calc([{ label: "", result }]))).toBe(false);
    expect(emptyRoof(calc([]))).toBe(true);
  });
});

describe("tower and arc refusals in the user's language", () => {
  it("names a tower's problem and an arc that does not fit", () => {
    i18n.locale = "en";
    expect(refusalReason("a curve may be at most half its chord and must not cross the tower's axis (tower_bulge)")).toBe(
      "a curve may be at most half its chord and must not cross the tower's axis",
    );
    expect(arcReason("the radius is too large for the walls at the corner (at most 1.250 m)")).toBe(
      "The radius is too large for the walls at the corner (at most 1.25 m).",
    );
    expect(arcReason("the bend must be at least 0.005 m and at most half the wall (2.000 m)")).toContain("(2 m)");
    expect(arcReason("something else")).toBeNull();
    i18n.locale = "cs";
    expect(refusalReason("the arc would make the outline invalid: wall 2 crosses wall 4")).toBe(
      "Oblouk by udělal obrys budovy neplatným (stěny by se křížily).",
    );
  });

  it("the placing bar says where to click for a tower", () => {
    expect(placeSteps("tower", 0)).toEqual({ title: "tool.tower", text: "plan.placeTower", counted: false });
  });
});
