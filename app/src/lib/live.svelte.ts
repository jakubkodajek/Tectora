// What the connected AI agent is doing right now (from `.activity/live.json`).

import { num } from "./format";
import { DICTS, t } from "./i18n.svelte";
import { ENGINE_ACTOR, RECEIVED_ENGINE_ACTOR, isAi, type ActivityItem, type LiveStatus } from "./types";

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
 * Who made a change-log entry, for the history and the library's job
 * panel: the AI client's name, "Tectora" for an entry the engine wrote
 * itself (a recalculated stored result), else the user (audit 2026-11-22
 * UX-704: the library credited the engine's entry to the user).
 */
export function actorLabel(actor: string): string {
  if (isAi(actor)) return actorName(actor);
  // An engine's entry that came with a bundle: the sender's Tectora, said so (ARCH-802).
  if (actor === RECEIVED_ENGINE_ACTOR) return t("ai.receivedEngine");
  return actor === ENGINE_ACTOR ? "Tectora" : t("ai.you");
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

/**
 * The editor's pill while the agent works outside this job: what it does
 * when that is not a job (settings, the library), otherwise "another job"
 * with the job's name, or without a colon when there is no name (UX-003).
 */
export function pillText(s: LiveStatus): string {
  if (!s.project_id) return liveText(s);
  return s.name ? t("ai.elsewhere", { name: s.name }) : t("ai.otherProject");
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

/**
 * The entry the core writes when it recalculates a stored result
 * (snapshot.rs `REFRESHED_PREFIX`, `refreshed_what`). It names what
 * changed: the roof area when that differs, else the codes of the lines
 * whose lengths differ, else "other figures" (audit 2026-11-22 UX-807).
 */
const REFRESHED_HEAD = "Stored result recalculated \\(Tectora (\\S+) -> (\\S+)\\): ";
const REFRESHED = new RegExp(`^${REFRESHED_HEAD}roof area (-?[\\d.]+) -> (-?[\\d.]+) m2$`);
const REFRESHED_LINES = new RegExp(`^${REFRESHED_HEAD}line lengths ([A-Za-z0-9]+(?:, [A-Za-z0-9]+)*)$`);
const REFRESHED_OTHER = new RegExp(`^${REFRESHED_HEAD}other figures$`);
/** Whether the summary is the engine's recalculation of a stored result. */
const REFRESHED_ANY = /^Stored result recalculated /;

/** The entry the core writes when the offer's result is put back (store.rs `RESTORED_PREFIX`). */
const RESTORED = /^Offer result restored \(Tectora (\S+) -> (\S+)\)(?:: roof area -?[\d.]+ -> -?[\d.]+ m2)?$/;

/** A recalculation's sentence under the i18n keys `<key>`, `<key>Lines` and `<key>Other`; null for any other summary. */
function refreshedText(key: "snap.refreshedNotice" | "chg.resultRefreshed", summary: string): string | null {
  const area = REFRESHED.exec(summary);
  if (area) {
    const [, from, to, a, b] = area;
    return t(key, { from, to, a: num(Number(a)), b: num(Number(b)) });
  }
  const lines = REFRESHED_LINES.exec(summary);
  if (lines) return t(`${key}Lines`, { from: lines[1], to: lines[2], codes: lines[3] });
  const other = REFRESHED_OTHER.exec(summary);
  if (other) return t(`${key}Other`, { from: other[1], to: other[2] });
  return null;
}

/**
 * The notice the editor shows when opening a job recalculated its stored
 * result (audit UX-702c): the engine's summary in the user's language.
 */
export function refreshedNotice(summary: string): string {
  return refreshedText("snap.refreshedNotice", summary) ?? entrySummary(summary);
}

/**
 * What the history shows for an entry: `entrySummary`, and for the user's
 * revert of AI changes the short "AI changes reverted". A revert of
 * Tectora's own recalculation, or of the user's own change, is no AI
 * change and keeps its sentence (audit 2026-11-22 UI-803); the undone
 * entry is looked up in `log` by the revert's `undid_from`.
 *
 * `recalculationTs`: the entry of the recalculation the job's stored
 * result still is (`Store::recalculation`). Only that entry says the
 * offer's figures can be put back (they can, from the result card and the
 * entry, audit 2026-11-23 HUNT-902); an older one, or one received with a
 * bundle, says nothing about it.
 */
export function historyText(
  a: Pick<ActivityItem, "actor" | "summary" | "undid_from" | "restores" | "reason"> & { ts_ms?: number },
  log: Pick<ActivityItem, "ts_ms" | "actor">[],
  recalculationTs?: number | null,
): string {
  if (a.actor === ENGINE_ACTOR && recalculationTs != null && a.ts_ms === recalculationTs && REFRESHED_ANY.test(a.summary)) {
    const text = refreshedText("chg.resultRefreshed", a.summary);
    if (text) return `${text} ${t("chg.refreshedBack")}`;
  }
  if (!isAi(a.actor) && a.restores == null && isRevert(a.summary)) {
    const undone = a.undid_from == null ? undefined : log.find((e) => e.ts_ms === a.undid_from);
    // Not in the list any more (or an entry of an older Tectora): told by the sentence.
    const ofAi = undone ? isAi(undone.actor) : !/Stored result recalculated \(Tectora /.test(a.summary);
    if (ofAi) return t("ai.revertedEntry");
  }
  return entrySummary(a.summary, a.reason);
}

/**
 * A change-log summary with the core's English revert sentences translated.
 * `reason`: the entry's own reason field (written since audit 2026-11-23
 * HUNT-904); the summary then ends with exactly ` (reason: <reason>)`, and
 * nothing is parsed out of the AI's free texts. Entries without the field
 * are split at the last ` (reason: `.
 */
export function entrySummary(summary: string, reason?: string | null): string {
  if (reason && summary.endsWith(`${REASON_OPEN}${reason})`)) {
    const head = summary.slice(0, -`${REASON_OPEN}${reason})`.length);
    const restored = RESTORED.exec(head);
    if (restored) return t("chg.revertReason", { s: t("chg.resultRestored", { to: restored[1], from: restored[2] }), r: reason });
    if (revertSentence(head)) return t("chg.revertReason", { s: revertText(head), r: reason });
  }
  const refreshed = REFRESHED_ANY.test(summary) ? refreshedText("chg.resultRefreshed", summary) : null;
  if (refreshed) return refreshed;
  if (summary.startsWith("Offer result restored ")) {
    // An AI's reason is the entry's last ` (reason: …)`.
    const at = summary.lastIndexOf(REASON_OPEN);
    const reason = at >= 0 && summary.endsWith(")") ? summary.slice(at + REASON_OPEN.length, -1) : null;
    const restored = RESTORED.exec(reason === null ? summary : summary.slice(0, at));
    if (restored) {
      const s = t("chg.resultRestored", { to: restored[1], from: restored[2] });
      return reason === null ? s : t("chg.revertReason", { s, r: reason });
    }
  }
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
