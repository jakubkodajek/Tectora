import { describe, expect, it } from "vitest";
import { hasBuilding, listedArea, offerDiffers, roofInput, snapshotDifferences, snapshotDiffers, storedArea } from "./offer";
import type { Project, ResultSnapshot } from "./types";

describe("offer figures in the library (UX-008)", () => {
  it("lists the stored area of an offer, not the current one", () => {
    const p = { has_result: true, stored_area: 148.77, result_differs: true, roof_area: 157.01 };
    expect(storedArea(p)).toBe(148.77);
    expect(listedArea(p)).toBe(148.77);
    expect(offerDiffers(p)).toBe(true);
  });

  it("lists the current area of a job without a stored result", () => {
    const p = { has_result: false, stored_area: null, result_differs: false, roof_area: 90 };
    expect(storedArea(p)).toBeNull();
    expect(listedArea(p)).toBe(90);
    expect(offerDiffers(p)).toBe(false);
  });

  it("marks nothing while the figures match", () => {
    expect(offerDiffers({ has_result: true, stored_area: 90, result_differs: false })).toBe(false);
  });
});

describe("what a stored result is calculated from (ERR-102, UX-102)", () => {
  const base = () =>
    ({
      schema_version: 1,
      name: "A",
      notes: "",
      job: { number: "", customer: "", address: "", status: "draft" },
      parts: [{ edges: [] }],
      dormers: [],
      chimneys: [],
    }) as unknown as Project;

  it("ignores the job texts, the status and the stored result", () => {
    const a = base();
    const b = { ...base(), name: "B", notes: "AI note", job: { ...a.job, status: "offer" }, result_snapshot: null } as Project;
    expect(roofInput(b)).toBe(roofInput(a));
  });

  it("sees a change of the roof", () => {
    const b = { ...base(), dormers: [{} as never] } as Project;
    expect(roofInput(b)).not.toBe(roofInput(base()));
  });

  // TEST-208: every input of the roof counts, not only the dormers.
  for (const field of ["parts", "chimneys", "windows", "roof_openings", "manual_faces", "manual_lines"] as const) {
    it(`sees a change of ${field}`, () => {
      const b = { ...base(), [field]: [{ edges: [], x: 1 } as never, {} as never] } as Project;
      expect(roofInput(b)).not.toBe(roofInput(base()));
    });
  }
  it("sees the flat roof switched on", () => {
    expect(roofInput({ ...base(), flat_roof: true } as Project)).not.toBe(roofInput(base()));
  });

  it("a job without a building and without a hand-drawn face has nothing to offer", () => {
    expect(hasBuilding(base())).toBe(true);
    expect(hasBuilding({ ...base(), parts: [] })).toBe(false);
    expect(hasBuilding({ ...base(), parts: [], manual_faces: [{} as never] })).toBe(true);
  });
});

describe("a stored offer against new figures (UX-206)", () => {
  const snap = (over: Partial<ResultSnapshot> = {}): ResultSnapshot => ({
    engine_version: "1",
    taken_ms: 0,
    roof_area: 129.24,
    ridge_height: 6.78,
    lines_by_code: { T: 20, OL: 11.75 },
    area_by_material: [{ material: 1, area: 129.24 }],
    ...over,
  });
  it("differs when any figure the core compares moves by 5 mm or more", () => {
    expect(snapshotDiffers(snap(), snap())).toBe(false);
    expect(snapshotDiffers(snap(), snap({ roof_area: 129.243 }))).toBe(false);
    expect(snapshotDiffers(snap(), snap({ roof_area: 120.4 }))).toBe(true);
    expect(snapshotDiffers(snap(), snap({ ridge_height: 6.08 }))).toBe(true);
    expect(snapshotDiffers(snap(), snap({ lines_by_code: { T: 20, OL: 11.75, K: 2 } }))).toBe(true);
    expect(snapshotDiffers(snap(), snap({ lines_by_code: { T: 20 } }))).toBe(true);
  });
  it("compares areas by material only when the roof has more than the default one", () => {
    expect(snapshotDiffers(snap(), snap({ area_by_material: [] }))).toBe(false);
    const split = [{ material: 1, area: 100 }, { material: 2, area: 29.24 }];
    expect(snapshotDiffers(snap({ area_by_material: split }), snap({ area_by_material: [{ material: 1, area: 90 }, { material: 2, area: 39.24 }] }))).toBe(true);
  });
});

// The same cases the core's `snapshot::compare` is tested with
// (crates/tectora-core/src/snapshot.rs `compare_matches_the_shared_cases`),
// so this copy of the rule cannot drift from it (TEST-302).
describe("snapshotDifferences follows the core's compare (shared cases)", () => {
  const files = import.meta.glob("../../../tests/fixtures/parity/snapshot_compare_cases.json", { eager: true, import: "default" }) as Record<string, unknown>;
  const file = Object.values(files)[0] as {
    base: Partial<ResultSnapshot>;
    cases: { name: string; stored: Partial<ResultSnapshot>; current: Partial<ResultSnapshot>; differs: string[] }[];
  };
  const snap = (over: Partial<ResultSnapshot>) => ({ engine_version: "test", taken_ms: 0, ...file.base, ...over }) as ResultSnapshot;
  it("has the cases", () => expect(file.cases.length).toBeGreaterThanOrEqual(10));
  for (const c of file.cases) {
    it(c.name, () => {
      const d = snapshotDifferences(snap(c.stored), snap(c.current));
      expect(d.map((x) => x.key)).toEqual(c.differs);
      expect(snapshotDiffers(snap(c.stored), snap(c.current))).toBe(c.differs.length > 0);
    });
  }
  it("gives both figures of a difference", () => {
    const d = snapshotDifferences(snap({}), snap({ area_by_material: [{ material: 1, area: 100 }, { material: 2, area: 29.24 }] }));
    expect(d).toEqual([
      { key: "material:1", stored: 129.24, current: 100 },
      { key: "material:2", stored: 0, current: 29.24 },
    ]);
  });
});
