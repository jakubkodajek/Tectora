// Turns errors from the Rust commands (and anything else that throws) into
// a translated message for the user plus the original text for "Details".

import { numberLocale, t } from "./i18n.svelte";
import { isDefaultPartName } from "./partname";
import type { CalcError, Problem, Project } from "./types";

/** An error as the Tauri commands return it (see app/src-tauri/src/error.rs). */
export interface AppError {
  code: string;
  id?: string;
  detail?: string;
}

/** What a failed action did, for the message title. */
export type Action =
  | "load"
  | "save"
  | "list"
  | "create"
  | "delete"
  | "duplicate"
  | "import"
  | "revert"
  | "revertAll"
  | "undo"
  | "redo"
  | "restoreOffer"
  | "accept"
  | "calc"
  | "preview"
  | "files"
  | "export"
  | "update"
  | "connect"
  | "restore"
  | "status"
  | "storeResult"
  | "settings"
  | "move"
  | "report"
  | "reportShow";

export interface Described {
  /** One sentence: what failed. */
  title: string;
  /** Why it failed and what to do, in the user's language. */
  message: string;
  /** The original (English) error text, shown under "Details". */
  detail?: string;
  code: string;
}

export function isAppError(e: unknown): e is AppError {
  return typeof e === "object" && e !== null && typeof (e as AppError).code === "string";
}

/** Normalizes anything thrown into an AppError. */
export function toAppError(e: unknown): AppError {
  if (isAppError(e)) return e;
  // The settings commands reject with a bare code ("not_writable"): keep it
  // as the code, not as the detail of an unknown error (ERRU-002).
  if (typeof e === "string" && (KNOWN.has(e) || SETTINGS_CODES.has(e))) return { code: e };
  const text = e instanceof Error ? e.message : typeof e === "string" ? e : JSON.stringify(e);
  // Tauri rejects arguments it cannot deserialize (an emptied number field
  // sends null) before our command runs.
  if (/^invalid args|invalid type: null|expected f64/i.test(text)) return { code: "invalid_input", detail: text };
  if (/not allowed|permission/i.test(text) && /plugin|command/i.test(text)) return { code: "not_allowed", detail: text };
  return { code: "unknown", detail: text || undefined };
}

const KNOWN = new Set([
  "invalid_id",
  "not_found",
  "unreadable",
  "newer_format",
  "library_changed",
  "permission_denied",
  "disk_full",
  "foreign_entry",
  "io",
  "nothing_to_revert",
  "revert_created",
  "no_such_change",
  "revert_conflict",
  "revert_stale",
  "review_stale",
  "revert_out_of_limits",
  "revert_out_of_limits_damaged",
  "revert_outside",
  "busy",
  "busy_edits",
  "busy_open",
  "left_before_action",
  "save_conflict",
  "not_saved",
  "unknown_example",
  "open_failed",
  "out_of_range",
  "file_not_found",
  "invalid_input",
  "not_allowed",
  "watcher",
  "internal",
  "update_failed",
  "no_update",
  "ai_not_installed",
  "ai_config_invalid",
  "ai_config_io",
  "too_large",
  "not_a_file",
  "unsupported_type",
  "invalid_name",
  "not_attachable",
  "too_many",
  "too_complex",
  "not_calculated",
  "no_building",
  "figures_too_large",
  "settings_unreadable",
  "result_dropped",
  "refused",
]);

/** Codes of the settings commands, with their messages under `settings.err.<code>`. */
const SETTINGS_CODES = new Set(["not_absolute", "not_a_folder", "not_writable", "invalid_value", "io"]);

/** The core's detail says a file is not UTF-8 text (store.rs `read_text`), not that it is damaged. */
export function notUtf8(detail: string | null | undefined): boolean {
  return /not UTF-8 text/.test(detail ?? "");
}

/** A translated description of an error from `action`. */
export function describeError(e: unknown, action?: Action): Described {
  const err = toAppError(e);
  const code = KNOWN.has(err.code) ? err.code : "unknown";
  // A settings code says what is wrong with the folder or value (ERRU-002).
  const settings = SETTINGS_CODES.has(err.code) && (action === "settings" || !KNOWN.has(err.code));
  // An export refused for the job's own size, not for its files (bundle.rs, ARCH-205).
  const jobTooLarge = code === "too_large" && /the job is too large to hand over/.test(err.detail ?? "");
  // Refused for its unknown keys (model.rs `check_counts`: their size or depth), not for its buildings or walls.
  const unknownKeys = code === "too_many" && /unknown keys/.test(err.detail ?? "");
  // A job file in another encoding is intact text, not a damaged file (store.rs `read_text`).
  const notText = code === "unreadable" && notUtf8(err.detail);
  return {
    code: err.code,
    title: t(action === "report" ? "err.ctx.report" : action === "reportShow" ? "err.ctx.reportShow" : action ? `err.action.${action}` : "err.title"),
    message: settings ? t(`settings.err.${err.code}`) : jobTooLarge ? t("err.job_too_large") : unknownKeys ? t("err.unknown_keys") : notText ? t("err.unreadable_text") : t(`err.${code}`, { id: err.id ?? "" }),
    detail: err.detail,
  };
}

/**
 * Why the library cannot read one of its files (store.rs `scan`), in the
 * user's language: the same text the editor gives when it opens that file
 * (a job refused for its unknown keys, a file in another encoding, ...),
 * not the operating system's refusal (ARCH-1303). A code this file does
 * not know is an input / output error.
 */
export function describeBroken(b: { id: string; code: string; detail?: string | null }): string {
  if (!KNOWN.has(b.code)) return t("err.io");
  return describeError({ code: b.code, id: b.id, detail: b.detail ?? undefined }).message;
}

/**
 * The roof calculation's error in the user's language. With several
 * buildings, the title names the ones whose roof fails on its own and the
 * hint says the others wait for them (one error stops every roof). Where
 * the core names two walls with parallel planes, the hint adds them.
 */
export function describeCalcError(e: CalcError, project?: Project): { title: string; hint: string; code: string } {
  const title = t(`calc.err.${e.kind}`);
  // The kind's own hint always; the parallel walls the core names for this
  // error come on top of it, never instead of it.
  const [a, b] = e.edges ?? [];
  const own = t(`calc.hint.${e.kind}`);
  const hint = a !== undefined && b !== undefined ? `${own} ${t("calc.hint.parallel", { a: a + 1, b: b + 1 })}` : own;
  const parts = e.parts ?? [];
  if (!project || project.parts.length < 2 || parts.length === 0) return { title, hint, code: e.kind };
  const names = parts.map((i) => {
    const name = project.parts[i]?.name;
    const where = t("prob.item.part", { n: i + 1 });
    return name && !isDefaultPartName(name) ? `${where} „${name}“` : where;
  });
  return { title: `${names.join(", ")} – ${title}`, hint: `${hint} ${t("calc.hint.othersWait")}`, code: e.kind };
}

/** One input problem, e.g. "Part 2 „Garage“, wall 3: pitch must be between 2° and 178°". */
export function describeProblem(p: Problem, project: Project): string {
  const n = p.index + 1;
  let where = t(`prob.item.${p.item}`, { n });
  if (p.item === "part" && project.parts.length > 1) {
    const name = project.parts[p.index]?.name;
    // A core default name ("Main") would only repeat "Budova 1".
    if (name && !isDefaultPartName(name)) where += ` „${name}“`;
  }
  if (p.edge !== undefined) where += `, ${t("prob.edge", { n: p.edge + 1 })}`;
  return `${where}: ${t(`prob.${p.code}`)}`;
}

/** What the user tried when the core refused a value (the toast's title). */
export type RefusedAction = "height" | "place" | "face" | "helper" | "transform";

const REFUSED_TITLE: Record<RefusedAction, string> = {
  height: "refuse.title.height",
  place: "place.refusedTitle",
  face: "refuse.title.face",
  helper: "refuse.title.helper",
  transform: "refuse.title.transform",
};

/**
 * The core's reason for refusing an edit (`code: "refused"`, its English
 * text in `detail`) in the user's language. The core says why in one of a
 * few sentences (`edit/height.rs`, `edit/position.rs`, `edit::face_to_part`);
 * anything else gets a general reason, the original stays under "Details".
 */
export function refusalReason(detail: string | undefined): string {
  const d = detail ?? "";
  if (/below the ground/.test(d)) return t("refuse.belowGround");
  if (/leave its roof face/.test(d)) return t("place.refused");
  if (/stands on no roof face/.test(d)) return t("refuse.noFace");
  if (/the helper gives values a wall does not take/.test(d)) return t("refuse.helperRange");
  const range = d.match(/out of range \((-?[\d.]+) to (-?[\d.]+) m/);
  if (range) {
    const f = (x: string) => new Intl.NumberFormat(numberLocale(), { useGrouping: false, maximumFractionDigits: 3 }).format(+x);
    return t("num.range", { min: f(range[1]), max: f(range[2]) });
  }
  return t("refuse.other");
}

/**
 * A value the core refused, described as a refusal (not as a failed
 * calculation); null when `e` is another error.
 */
export function describeRefusal(e: unknown, action: RefusedAction): Described | null {
  const err = toAppError(e);
  if (err.code !== "refused") return null;
  return { code: "refused", title: t(REFUSED_TITLE[action]), message: refusalReason(err.detail), detail: err.detail };
}

/**
 * A dormer the core refused to store (`check_dormer`: `dormer_below_roof`,
 * `dormer_pitch`, `dormer_segments`), as the dormer field's reason in the user's language;
 * null for any other error (MATH-001, MATH-003).
 */
export function dormerRefusal(e: unknown): string | null {
  const code = toAppError(e).code;
  if (code === "dormer_below_roof") return t("dormer.err.eave_below_roof");
  if (code === "dormer_pitch") return t("dormer.err.bad_pitch");
  if (code === "dormer_segments") return t("dormer.err.segments");
  return null;
}
