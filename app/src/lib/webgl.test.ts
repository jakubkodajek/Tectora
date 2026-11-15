import { describe, expect, it } from "vitest";
import { releaseRenderer } from "./webgl";

describe("releaseRenderer (ERRU-008)", () => {
  it("disposes, loses the WebGL context and removes the canvas", () => {
    const calls: string[] = [];
    releaseRenderer({
      dispose: () => calls.push("dispose"),
      forceContextLoss: () => calls.push("lose"),
      domElement: { remove: () => calls.push("remove") },
    });
    expect(calls).toEqual(["dispose", "lose", "remove"]);
  });
});
