// What the connected AI agent is doing right now (from `.activity/live.json`).

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
