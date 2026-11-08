import { describe, expect, it } from "vitest";
import { hasUnsaved, mayLeave, releaseLeaveGuard, setLeaveGuard } from "./leave";

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
});
