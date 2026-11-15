import { describe, expect, it } from "vitest";
import { listedArea, offerDiffers, storedArea } from "./offer";

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
