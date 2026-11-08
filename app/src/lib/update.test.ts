import { describe, expect, it } from "vitest";
import { installQuestion, installStatus } from "./update";

const t = (key: string, params: Record<string, string | number> = {}) =>
  `${key}${Object.entries(params).map(([k, v]) => ` ${k}=${v}`).join("")}`;

describe("update", () => {
  it("asks with the release notes when there are any", () => {
    expect(installQuestion(t, { version: "0.2.0", notes: "" })).toBe("update.confirm version=0.2.0");
    expect(installQuestion(t, { version: "0.2.0", notes: "- Faster" })).toBe(
      "update.confirm version=0.2.0\n\nupdate.notes\n- Faster",
    );
  });

  it("reports the download, then the install", () => {
    expect(installStatus(t, "0.2.0", null)).toBe("update.downloading version=0.2.0");
    expect(installStatus(t, "0.2.0", { downloaded: 10, total: null })).toBe("update.downloading version=0.2.0");
    expect(installStatus(t, "0.2.0", { downloaded: 499, total: 1000 })).toBe(
      "update.downloadingPercent version=0.2.0 percent=49",
    );
    expect(installStatus(t, "0.2.0", { downloaded: 1000, total: 1000 })).toBe("update.installing version=0.2.0");
  });
});
