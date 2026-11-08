import { describe, expect, it } from "vitest";
import { hasUnsaved, isBusy, mayLeave, releaseLeaveGuard, setLeaveGuard } from "./leave";

describe("leave guard", () => {
  it("lets go when no editor is open", async () => {
    setLeaveGuard(null);
    expect(await mayLeave()).toBe(true);
    expect(hasUnsaved()).toBe(false);
  });

  it("asks the open editor, with the mode", async () => {
    const modes: string[] = [];
    setLeaveGuard(
      async (m) => {
        modes.push(m);
        return m === "ask";
      },
      () => true,
    );
    expect(await mayLeave()).toBe(true);
    expect(await mayLeave("quiet")).toBe(false);
    expect(modes).toEqual(["ask", "quiet"]);
    expect(hasUnsaved()).toBe(true);
    setLeaveGuard(null);
    expect(hasUnsaved()).toBe(false);
  });

  it("an editor that closes after a newer one opened leaves the newer guard", async () => {
    const older = async () => true;
    const newer = async () => false;
    setLeaveGuard(older);
    setLeaveGuard(newer);
    releaseLeaveGuard(older);
    expect(await mayLeave()).toBe(false);
    releaseLeaveGuard(newer);
    expect(await mayLeave()).toBe(true);
  });

  it("follow mode waits while the user is busy (UX-001)", () => {
    const doc = (dialog: boolean, active: { tagName: string; type?: string } | null) =>
      ({ querySelector: () => (dialog ? {} : null), activeElement: active }) as unknown as Document;
    setLeaveGuard(null);
    expect(isBusy(doc(false, null))).toBe(false);
    expect(isBusy(doc(true, null))).toBe(true);
    expect(isBusy(doc(false, { tagName: "INPUT", type: "text" }))).toBe(true);
    expect(isBusy(doc(false, { tagName: "TEXTAREA" }))).toBe(true);
    expect(isBusy(doc(false, { tagName: "BUTTON" }))).toBe(false);
    const field = (typing: boolean) => ({ tagName: "INPUT", type: "text", hasAttribute: (n: string) => n === "data-field" || (typing && n === "data-typing") });
    expect(isBusy(doc(false, field(false))), "a focused field with nothing typed").toBe(false);
    expect(isBusy(doc(false, field(true))), "typed text not stored yet").toBe(true);
    let drawing = true;
    setLeaveGuard(async () => true, () => false, () => drawing);
    expect(isBusy(doc(false, null))).toBe(true);
    drawing = false;
    expect(isBusy(doc(false, null))).toBe(false);
    expect(isBusy(doc(true, null))).toBe(true);
    setLeaveGuard(null);
  });
});
