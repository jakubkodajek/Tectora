// What the connected AI agent is doing right now (from `.activity/live.json`).

import { num } from "./format";
import { DICTS, t } from "./i18n.svelte";
import { ENGINE_ACTOR, RECEIVED_ENGINE_ACTOR, USER_ACTOR, isAi, type ActivityItem, type EntryKind, type LiveStatus } from "./types";

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
  // A change's message is the agent's own summary: its words, quoted (ARCH-1401).
  if (s.kind === "changed") return t("ai.quoted", { s: s.message });
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
/** Its start, for an entry the store cut (`cutHead`). */
const RESTORED_START = /^Offer result restored \(Tectora (\S+) -> (\S+)\)/;

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
 * The recalculation of the stored result a received job came with (audit
 * 2026-11-23 UX-1003; owner decision: bundles carry no offer figures): the
 * newest entry that changed the stored result is the sender's engine's
 * (`tectora (received)`), and the job's result is of the engine it names.
 * The card shows it as information, without the put-back; `area` is the
 * offer's roof area when the entry names it.
 */
export function receivedRecalculation(
  log: readonly Pick<ActivityItem, "ts_ms" | "actor" | "summary" | "changes">[],
  project: { result_snapshot?: { engine_version: string } | null } | null,
): { at: number; from: string; to: string; area: number | null } | null {
  const engine = project?.result_snapshot?.engine_version;
  if (!engine) return null;
  const last = log.find((a) => a.changes.some((c) => c.field === "result_snapshot"));
  if (!last || last.actor !== RECEIVED_ENGINE_ACTOR) return null;
  const head = new RegExp(`^${REFRESHED_HEAD}`).exec(last.summary);
  if (!head || head[2] !== engine) return null;
  const area = REFRESHED.exec(last.summary);
  return { at: last.ts_ms, from: head[1], to: head[2], area: area ? Number(area[3]) : null };
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
  a: Pick<ActivityItem, "actor" | "summary" | "undid_from" | "restores" | "reason" | "kind"> & { ts_ms?: number },
  log: readonly LoggedEntry[],
  recalculationTs?: number | null,
): string {
  const kind = entryKind(a);
  if (kind === "refresh" && a.actor === ENGINE_ACTOR && recalculationTs != null && a.ts_ms === recalculationTs) {
    const text = refreshedText("chg.resultRefreshed", a.summary);
    if (text) return `${text} ${t("chg.refreshedBack")}`;
  }
  if (kind === "revert" && !isAi(a.actor)) {
    const undone = a.undid_from == null ? undefined : log.find((e) => e.ts_ms === a.undid_from);
    // The user's take-back of a revert of AI changes (the editor's Undo of
    // a Revert, UX-1601): the AI's changes stand again.
    if (undone && entryKind(undone) === "revert" && !isAi(undone.actor)) {
      const inner = undone.undid_from == null ? undefined : log.find((e) => e.ts_ms === undone.undid_from);
      if (inner && isAi(inner.actor)) return t("ai.revertTakenBack");
    }
    // Not in the list any more (or the user's revert of an older Tectora): told by the sentence.
    const ofAi = undone ? isAi(undone.actor) : !/Stored result recalculated \(Tectora /.test(a.summary);
    if (ofAi) {
      // One AI change undone: named, as the toast of the same press (UX-1603).
      if (undone && a.summary.startsWith(REVERTED)) {
        const words = undone.summary != null ? entryText(undone as EntryLike, log) : t("ai.quoted", { s: a.summary.slice(REVERTED.length) });
        return t("ai.revertedOne", { s: words });
      }
      return t("ai.revertedEntry");
    }
  }
  return entryText(a, log);
}

/** A change-log entry as `entryText` reads it. */
export type EntryLike = Pick<ActivityItem, "actor" | "summary"> & {
  kind?: EntryKind | null;
  reason?: string | null;
  undid_from?: number;
  restores?: number;
};
/** An entry of the list a revert's undone entry is looked up in (by `undid_from`). */
export type LoggedEntry = Pick<ActivityItem, "ts_ms" | "actor"> & Partial<EntryLike>;

/**
 * What Tectora wrote an entry as: the core's `kind` (store.rs
 * `ActivityEntry::kind`); for an item without it (an older bridge), the
 * same rule from its fields. Never from an AI's words: an AI's summary
 * that reads like a revert is a change (audit 2026-11-23 ARCH-1401). The
 * words count only for the user's own reverts of Tectora 0.8.0 and older,
 * which did not record `undid_from`; an AI client never logs as the user.
 */
export function entryKind(a: Pick<EntryLike, "actor" | "kind" | "undid_from" | "restores"> & { summary?: string }): EntryKind | null {
  // The core always sends the field, `null` for a change (ARCH-1601): the
  // rule below is only for an item without it (an older bridge).
  if (a.kind !== undefined) return a.kind;
  const summary = a.summary ?? "";
  if (a.restores != null) return "restore";
  if (a.undid_from != null || (a.actor === USER_ACTOR && revertSentence(summary))) return "revert";
  if ((a.actor === ENGINE_ACTOR || a.actor === RECEIVED_ENGINE_ACTOR) && REFRESHED_ANY.test(summary)) return "refresh";
  return null;
}

/** How deep a revert's quoted entry is followed back through the list. */
const MAX_NESTED = 8;

/**
 * What the history, the review banner and the library card show for an
 * entry: Tectora's own sentence in the user's language when the entry is
 * a revert, a put-back or a recalculation by its kind (`entryKind`), else
 * the summary as written, an AI's in quotation marks as its own words
 * (audit 2026-11-23 ARCH-1401: an AI wrote "Reverted: …" or the engine's
 * sentence and it read as Tectora's). The entry a revert quotes is
 * translated only when it is found in `log` by `undid_from` and has a kind
 * itself; a user's or the engine's change found there is shown as it is,
 * and every other quoted part (an AI's change, or one not found) in
 * quotation marks (ARCH-1501).
 */
export function entryText(a: EntryLike, log: readonly LoggedEntry[] = [], depth = 0): string {
  const kind = entryKind(a);
  if (kind === null) return isAi(a.actor) ? t("ai.quoted", { s: a.summary }) : a.summary;
  return coreSentence(a.summary, a.reason, kind, (inner) => {
    const undone = a.undid_from == null || depth >= MAX_NESTED ? undefined : log.find((e) => e.ts_ms === a.undid_from);
    // The quoted part is shown as Tectora's only when the undone entry is
    // found and is Tectora's own (it has a kind, or the user or the engine
    // wrote it); an AI's change, or one not in the list (the library card,
    // a trimmed log), keeps its words in quotation marks (audit 2026-11-23
    // ARCH-1501).
    if (!undone) return t("ai.quoted", { s: inner });
    // The words as the revert quoted them, the fields of the entry it undid.
    const u: EntryLike = { ...undone, summary: inner };
    if (entryKind(u)) return entryText(u, log, depth + 1);
    return isAi(u.actor) ? t("ai.quoted", { s: inner }) : inner;
  });
}

/**
 * A summary Tectora itself wrote, with the core's English sentences
 * translated, entries a revert quotes too. Only for a text known to be
 * the core's (the recalculation notice on open); the history, the banner
 * and the card go by the entry's kind (`entryText`).
 * `reason`: the entry's own reason field (written since audit 2026-11-23
 * HUNT-904); the summary then ends with exactly ` (reason: <reason>)`, and
 * nothing is parsed out of the AI's free texts. Entries without the field
 * are split at the last ` (reason: `.
 */
export function entrySummary(summary: string, reason?: string | null): string {
  return coreSentence(summary, reason, null, (inner) => entrySummary(inner));
}

/**
 * `summary` as the core's sentence of `kind` (any of them when null) in
 * the user's language, the entry a revert quotes shown by `nested`; the
 * summary as written when it is not that sentence.
 */
function coreSentence(summary: string, reason: string | null | undefined, kind: EntryKind | null, nested: (s: string) => string): string {
  const restore = kind === null || kind === "restore";
  const revert = kind === null || kind === "revert";
  if (reason && summary.endsWith(`${REASON_OPEN}${reason})`)) {
    const head = summary.slice(0, -`${REASON_OPEN}${reason})`.length);
    const restored = restore ? RESTORED.exec(head) : null;
    if (restored) return t("chg.revertReason", { s: t("chg.resultRestored", { to: restored[1], from: restored[2] }), r: reason });
    if (revert && revertSentence(head)) return t("chg.revertReason", { s: revertText(head, nested), r: reason });
  }
  const cut = reason ? cutHead(summary, reason) : null;
  if (cut !== null && reason) {
    const restored = restore ? RESTORED_START.exec(cut) : null;
    if (restored) return t("chg.revertReason", { s: t("chg.resultRestored", { to: restored[1], from: restored[2] }), r: reason });
    if (revert && revertSentence(cut)) return t("chg.revertReason", { s: revertText(cut, nested), r: reason });
  }
  const refreshed = (kind === null || kind === "refresh") && REFRESHED_ANY.test(summary) ? refreshedText("chg.resultRefreshed", summary) : null;
  if (refreshed) return refreshed;
  if (restore && summary.startsWith("Offer result restored ")) {
    // An AI's reason is the entry's last ` (reason: …)`.
    const at = summary.lastIndexOf(REASON_OPEN);
    const reason = at >= 0 && summary.endsWith(")") ? summary.slice(at + REASON_OPEN.length, -1) : null;
    const restored = RESTORED.exec(reason === null ? summary : summary.slice(0, at));
    if (restored) {
      const s = t("chg.resultRestored", { to: restored[1], from: restored[2] });
      return reason === null ? s : t("chg.revertReason", { s, r: reason });
    }
  }
  if (!revert) return summary;
  if (revertSentence(summary)) {
    // The AI's reason is the entry's last ` (reason: …)`.
    const at = summary.lastIndexOf(REASON_OPEN);
    if (at >= 0 && summary.endsWith(")")) {
      return t("chg.revertReason", { s: revertText(summary.slice(0, at), nested), r: summary.slice(at + REASON_OPEN.length, -1) });
    }
    return revertText(summary, nested);
  }
  const legacy = LEGACY_REASON.exec(summary);
  if (legacy) return t("chg.revertReason", { s: revertText(legacy[2], nested), r: legacy[1] });
  return summary;
}

/** The mark the store ends a capped text with (edit `capped`). */
const CUT = "…";

/**
 * The core's sentence of an entry whose summary the store capped at 500
 * characters before it kept the reason whole (audit 2026-11-23 HUNT-1002,
 * ERR-1003): the cut fell inside the reason (the head is the text before
 * its ` (reason: `), or inside the sentence before it (the head is the
 * sentence as cut, ending in "…", without a begun ` (reason: `). Null
 * when the summary was not cut.
 */
function cutHead(summary: string, reason: string): string | null {
  if (!summary.endsWith(CUT)) return null;
  const body = summary.slice(0, -CUT.length);
  const at = body.lastIndexOf(REASON_OPEN);
  // The text after the last ` (reason: ` is the start of the reason, not an AI's own "(reason: …)".
  if (at >= 0 && reason.startsWith(body.slice(at + REASON_OPEN.length))) return body.slice(0, at);
  for (let k = REASON_OPEN.length - 1; k > 0; k--) {
    if (body.endsWith(REASON_OPEN.slice(0, k))) return `${body.slice(0, -k)}${CUT}`;
  }
  return summary;
}

/** A revert's sentence (without a reason) in the user's language, the entry it quotes shown by `nested`. */
function revertText(summary: string, nested: (s: string) => string): string {
  if (summary.startsWith(REVERTED)) return t("chg.revertedEntry", { s: nested(summary.slice(REVERTED.length)) });
  const m = REVERTED_MANY.exec(summary);
  if (m) return t("chg.revertedMany", { n: m[1], s: nested(summary.slice(m[0].length)) });
  return summary;
}
