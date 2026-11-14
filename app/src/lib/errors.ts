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
  | "calc"
  | "preview"
  | "files"
  | "export"
  | "update"
  | "connect";

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
  "io",
  "nothing_to_revert",
  "revert_created",
  "no_such_change",
  "revert_conflict",
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
  "too_many",
  "not_calculated",
  "settings_unreadable",
  "refused",
]);

/** A translated description of an error from `action`. */
export function describeError(e: unknown, action?: Action): Described {
  const err = toAppError(e);
  const code = KNOWN.has(err.code) ? err.code : "unknown";
  return {
    code: err.code,
    title: t(action ? `err.action.${action}` : "err.title"),
    message: t(`err.${code}`, { id: err.id ?? "" }),
    detail: err.detail,
  };
}

/**
 * The roof calculation's error in the user's language. With several
 * buildings, the title names the ones whose roof fails on its own and the
 * hint says the others wait for them (one error stops every roof).
 */
export function describeCalcError(e: CalcError, project?: Project): { title: string; hint: string; code: string } {
  const title = t(`calc.err.${e.kind}`);
  const hint = t(`calc.hint.${e.kind}`);
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
export type RefusedAction = "height" | "place" | "face";

const REFUSED_TITLE: Record<RefusedAction, string> = {
  height: "refuse.title.height",
  place: "place.refusedTitle",
  face: "refuse.title.face",
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
