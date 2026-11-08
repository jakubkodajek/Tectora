// Turns errors from the Rust commands (and anything else that throws) into
// a translated message for the user plus the original text for "Details".

import { t } from "./i18n.svelte";
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
  | "update";

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
  "permission_denied",
  "disk_full",
  "io",
  "nothing_to_revert",
  "revert_created",
  "no_such_change",
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

/** The roof calculation's error in the user's language. */
export function describeCalcError(e: CalcError): { title: string; hint: string; code: string } {
  return { title: t(`calc.err.${e.kind}`), hint: t(`calc.hint.${e.kind}`), code: e.kind };
}

/** One input problem, e.g. "Part 2 „Garage“, wall 3: pitch must be between 0° and 180°". */
export function describeProblem(p: Problem, project: Project): string {
  const n = p.index + 1;
  let where = t(`prob.item.${p.item}`, { n });
  if (p.item === "part" && project.parts.length > 1) {
    const name = project.parts[p.index]?.name;
    if (name) where += ` „${name}“`;
  }
  if (p.edge !== undefined) where += `, ${t("prob.edge", { n: p.edge + 1 })}`;
  return `${where}: ${t(`prob.${p.code}`)}`;
}
