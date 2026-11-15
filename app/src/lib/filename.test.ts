import { describe, expect, it } from "vitest";
import { safeFileName } from "./filename";

describe("safeFileName (UX-006)", () => {
  it("replaces characters macOS or Windows refuse in a file name", () => {
    expect(safeFileName("Novák / Brno 2026:117")).toBe("Novák _ Brno 2026_117");
    expect(safeFileName('a\\b*c?d"e<f>g|h')).toBe("a_b_c_d_e_f_g_h");
    expect(safeFileName("tab\there\nline")).toBe("tab_here_line");
  });

  it("drops trailing dots and spaces Windows would strip, and a leading dot", () => {
    expect(safeFileName("Dům u lesa. ")).toBe("Dům u lesa");
    expect(safeFileName(".hidden")).toBe("hidden");
  });

  it("never proposes a Windows device name", () => {
    expect(safeFileName("CON")).toBe("_CON");
    expect(safeFileName("nul.txt")).toBe("_nul.txt");
    expect(safeFileName("Console")).toBe("Console");
  });

  it("falls back when nothing usable is left, and stays short", () => {
    expect(safeFileName("")).toBe("job");
    expect(safeFileName(" / ", "roof")).toBe("roof");
    expect(safeFileName(null)).toBe("job");
    expect(safeFileName("x".repeat(300)).length).toBeLessThanOrEqual(120);
  });
});
