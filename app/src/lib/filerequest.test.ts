import { describe, expect, it } from "vitest";
import { fileRequestPurpose } from "./filerequest";
import { DICTS } from "./i18n.svelte";

describe("file request of an AI client (ARCH-002)", () => {
  it("has a sentence for every purpose the MCP server sends, and for an unknown one", () => {
    expect(fileRequestPurpose("read_drawing")).toBe("fileReq.readDrawing");
    expect(fileRequestPurpose("attach")).toBe("fileReq.attach");
    expect(fileRequestPurpose("import")).toBe("fileReq.import");
    expect(fileRequestPurpose("something new")).toBe("fileReq.read");
  });

  it("reads a prototype name from a hand-edited settings file as plain read (TEST-1403)", () => {
    for (const p of ["constructor", "toString", "__proto__", "hasOwnProperty", "valueOf"]) expect(fileRequestPurpose(p), p).toBe("fileReq.read");
  });

  it("names the AI client in every language", () => {
    for (const [locale, dict] of Object.entries(DICTS)) {
      for (const p of ["read_drawing", "attach", "import", "x"]) {
        const text = (dict as Record<string, string>)[fileRequestPurpose(p)];
        expect(text, `${locale} ${p}`).toContain("{by}");
      }
    }
  });
});
