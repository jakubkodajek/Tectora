// What the connected AI agent is doing right now (from `.activity/live.json`).

import { DICTS, t } from "./i18n.svelte";
import type { LiveStatus } from "./types";

/** A status older than this is shown as idle. */
export const LIVE_FRESH_MS = 90_000;

export const live = $state<{ status: LiveStatus | null; now: number }>({ status: null, now: Date.now() });

export function isFresh(): boolean {
  return !!live.status && live.now - live.status.ts_ms < LIVE_FRESH_MS;
}

/** Human name for an MCP client id such as "claude-ai" or "gemini-cli-mcp-client". */
export function actorName(actor: string): string {
  const a = actor.toLowerCase();
  if (a.includes("claude")) return "Claude";
  if (a.includes("gemini")) return "Gemini";
  if (a.includes("chatgpt") || a.includes("openai") || a.includes("codex")) return "ChatGPT";
  if (a === "ai" || a === "") return "AI";
  return actor;
}

/**
 * What the agent is doing, in the user's language: the MCP server sends a
 * code and a name; a change carries the agent's own summary. Older servers
 * send English text only, shown as it is.
 */
export function liveText(s: LiveStatus): string {
  if (s.kind && s.kind !== "changed" && `ai.live.${s.kind}` in DICTS.en) return t(`ai.live.${s.kind}`, { name: s.name ?? "" });
  return s.message;
}

/** Prefix the core writes before the summary a revert undid (store.rs `REVERTED_PREFIX`). */
const REVERTED = "Reverted: ";
const REVERTED_MANY = /^Reverted (\d+) changes since: /;
/** What follows a revert's sentence when the AI gave a reason (store.rs `REVERT_REASON_OPEN`). */
const REASON_OPEN = " (reason: ";
/** Entries written before that fixed form: `reason (Reverted: …)`. */
const LEGACY_REASON = /^(.*?) \((Reverted: .*|Reverted \d+ changes since: .*)\)$/s;

function revertSentence(summary: string): boolean {
  return summary.startsWith(REVERTED) || REVERTED_MANY.test(summary);
}

/** Whether a change-log summary is one the core wrote for a revert. */
export function isRevert(summary: string): boolean {
  return revertSentence(summary) || LEGACY_REASON.test(summary);
}

/** A change-log summary with the core's English revert sentences translated. */
export function entrySummary(summary: string): string {
  if (revertSentence(summary)) {
    // The AI's reason is the entry's last ` (reason: …)`.
    const at = summary.lastIndexOf(REASON_OPEN);
    if (at >= 0 && summary.endsWith(")")) {
      return t("chg.revertReason", { s: revertText(summary.slice(0, at)), r: summary.slice(at + REASON_OPEN.length, -1) });
    }
    return revertText(summary);
  }
  const legacy = LEGACY_REASON.exec(summary);
  if (legacy) return t("chg.revertReason", { s: revertText(legacy[2]), r: legacy[1] });
  return summary;
}

/** A revert's sentence (without a reason) in the user's language. */
function revertText(summary: string): string {
  if (summary.startsWith(REVERTED)) return t("chg.revertedEntry", { s: entrySummary(summary.slice(REVERTED.length)) });
  const m = REVERTED_MANY.exec(summary);
  if (m) return t("chg.revertedMany", { n: m[1], s: entrySummary(summary.slice(m[0].length)) });
  return summary;
}
