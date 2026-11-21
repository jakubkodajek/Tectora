import { describe, expect, it } from "vitest";
import { hasBuilding, listedArea, offerDiffers, roofInput, storedArea } from "./offer";
import type { Project } from "./types";

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

  it("a job without a building and without a hand-drawn face has nothing to offer", () => {
    expect(hasBuilding(base())).toBe(true);
    expect(hasBuilding({ ...base(), parts: [] })).toBe(false);
    expect(hasBuilding({ ...base(), parts: [], manual_faces: [{} as never] })).toBe(true);
  });
});
