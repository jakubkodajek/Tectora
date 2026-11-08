import { describe, expect, it } from "vitest";
import { DICTS, i18n } from "./i18n.svelte";
import { entrySummary, isRevert, liveText } from "./live.svelte";

// The codes the MCP server sends (crates/tectora-core/src/store.rs LIVE_KINDS).
const KINDS = [
  "settings",
  "browsing",
  "reading",
  "calculating",
  "explaining",
  "comparing",
  "showing",
  "reverted",
  "reverted_changes",
  "copied",
  "deleted",
  "restored",
  "attached",
  "removed",
  "exported",
  "exported_sheet",
  "imported",
];

const status = (kind: string | undefined, message: string, name?: string) => ({ ts_ms: 0, actor: "claude", message, project_id: "a", kind, name });

describe("liveText (UX-002)", () => {
  it("has every status sentence in all four languages", () => {
    for (const locale of Object.keys(DICTS) as (keyof typeof DICTS)[]) {
      for (const k of KINDS) expect(DICTS[locale][`ai.live.${k}`], `${locale} ${k}`).toBeTruthy();
    }
  });

  it("translates the code and keeps the agent's own summary for a change", () => {
    i18n.locale = "cs";
    expect(liveText(status("reading", "Reading “Sedlo”", "Sedlo"))).toBe("Čte „Sedlo“");
    expect(liveText(status("changed", "Sklon 42° podle řezu"))).toBe("Sklon 42° podle řezu");
    // An older server without a code, or a code this app does not know.
    expect(liveText(status(undefined, "Reading “Sedlo”"))).toBe("Reading “Sedlo”");
    expect(liveText(status("dancing", "Dancing"))).toBe("Dancing");
  });
});

describe("entrySummary", () => {
  it("translates the revert entries the core writes, nested too", () => {
    i18n.locale = "cs";
    expect(entrySummary("Reverted: Sklon 45°")).toBe("Vráceno: Sklon 45°");
    expect(entrySummary("Reverted 2 changes since: Sklon 45°")).toBe("Vráceno změn: 2, od: Sklon 45°");
    expect(entrySummary("Reverted: Reverted: X")).toBe("Vráceno: Vráceno: X");
    expect(entrySummary("Sklon 45°")).toBe("Sklon 45°");
    expect(isRevert("Reverted 3 changes since: x") && isRevert("Reverted: x") && !isRevert("Revert the eave")).toBe(true);
    i18n.locale = "de";
    expect(entrySummary("Reverted: Pitch")).toBe("Rückgängig: Pitch");
  });

  it("translates a revert with the AI's reason, in every language (UX-002)", () => {
    const want: Record<string, string> = {
      cs: "Vráceno: na část (důvod: zpět)",
      sk: "Vrátené: na část (dôvod: zpět)",
      en: "Reverted: na část (reason: zpět)",
      de: "Rückgängig: na část (Grund: zpět)",
    };
    for (const [loc, text] of Object.entries(want)) {
      i18n.locale = loc as typeof i18n.locale;
      expect(entrySummary("Reverted: na část (reason: zpět)")).toBe(text);
    }
    i18n.locale = "cs";
    expect(entrySummary("Reverted 2 changes since: X (reason: back)")).toBe("Vráceno změn: 2, od: X (důvod: back)");
    // A revert of a revert with reasons: the outer reason is the last one.
    expect(entrySummary("Reverted: Reverted: X (reason: a) (reason: b)")).toBe("Vráceno: Vráceno: X (důvod: a) (důvod: b)");
    // Entries written before the fixed form: `reason (Reverted: …)`.
    expect(entrySummary("undo (Reverted: to part)")).toBe("Vráceno: to part (důvod: undo)");
    expect(isRevert("undo (Reverted: to part)")).toBe(true);
    // An AI summary that only mentions a reason is left as it is.
    expect(entrySummary("Pitch 45 (reason: drawing)")).toBe("Pitch 45 (reason: drawing)");
  });
});
