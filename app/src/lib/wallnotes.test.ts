import { beforeEach, describe, expect, it } from "vitest";
import { i18n } from "./i18n.svelte";
import type { Change } from "./types";
import { wallNotes } from "./wallnotes";

const mod = (field: string, part: number, edge: number, before: unknown, after: unknown): Change => ({ kind: "modified", field, part, edge, before, after });

describe("wallNotes: the change written next to an AI-changed wall (UX-017)", () => {
  beforeEach(() => {
    i18n.locale = "en";
  });
  it("notes pitch, eave / gable, overhang and eave height per wall", () => {
    const n = wallNotes([
      mod("pitch_deg", 0, 0, 40, 35),
      mod("pitch_deg", 0, 1, 40, 90),
      mod("pitch_deg", 0, 2, 90, 45),
      mod("overhang", 0, 3, 0.5, 0.8),
      mod("eave_height", 0, 3, 3, 3.4),
    ]);
    expect(n.get("0:0")).toBe("40° → 35°");
    expect(n.get("0:1")).toBe("40° → Gable");
    expect(n.get("0:2")).toBe("Gable → 45°");
    expect(n.get("0:3")).toBe("Eave height 3.00 → 3.40 m · Overhang 0.50 → 0.80 m");
  });
  it("spans several AI steps (newest first) and drops a field that ended where it began", () => {
    const n = wallNotes([mod("pitch_deg", 1, 2, 35, 30), mod("pitch_deg", 1, 2, 40, 35), mod("overhang", 1, 2, 0.8, 0.5), mod("overhang", 1, 2, 0.5, 0.8)]);
    expect(n.get("1:2")).toBe("40° → 30°");
    expect(wallNotes([mod("pitch_deg", 0, 0, 35, 40), mod("pitch_deg", 0, 0, 40, 35)]).size).toBe(0);
  });
  it("ignores other fields, objects, added walls and bad values", () => {
    const n = wallNotes([
      mod("material", 0, 0, 1, 2),
      { kind: "modified", field: "pitch_deg", item: 0, before: 40, after: 30 },
      { kind: "added", field: "edge", part: 0, edge: 1, before: null, after: null },
      mod("pitch_deg", 0, 2, "x", 30),
      mod("pitch_deg", 0, 3, 40, null),
    ]);
    expect(n.size).toBe(0);
  });
  it("is translated", () => {
    i18n.locale = "cs";
    expect(wallNotes([mod("pitch_deg", 0, 0, 40, 90)]).get("0:0")).toBe("40° → Štít");
  });
});
