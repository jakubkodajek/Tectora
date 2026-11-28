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
  | "move";

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
  // Over the towers' limits (a tower duplicated, `edit::towers::check_tower_counts`): that limit, not "too many objects".
  const towerLimit = code === "too_many" ? err.detail?.match(/\((tower_limit|tower_tiers_limit)\)\s*$/)?.[1] : undefined;
  if (towerLimit) {
    const text = t(`prob.${towerLimit}`);
    return { code: err.code, title: t(action ? `err.action.${action}` : "err.title"), message: text.charAt(0).toLocaleUpperCase() + text.slice(1) + ".", detail: err.detail };
  }
  return {
    code: err.code,
    title: t(action ? `err.action.${action}` : "err.title"),
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
/** Towers by their names, else their numbers ("Věž 1, Lucerna"). */
export function towerNames(towers: number[], project?: Pick<Project, "towers"> | null): string {
  return towers.map((i) => project?.towers?.[i]?.label || t("tower.n", { n: i + 1 })).join(", ");
}

export function describeCalcError(e: CalcError, project?: Project): { title: string; hint: string; code: string } {
  // A tower's shaft reaches the core as a chimney: when the core names the
  // towers to blame, say so instead of sending the user to a chimney (ERR-005).
  const shafts = e.kind === "non_convex_opening" ? (e.towers ?? []) : [];
  const title = shafts.length ? t("calc.err.tower_shaft", { names: towerNames(shafts, project) }) : t(`calc.err.${e.kind}`);
  // The kind's own hint always; the parallel walls the core names for this
  // error come on top of it, never instead of it.
  const [a, b] = e.edges ?? [];
  const own = shafts.length ? t("calc.hint.tower_shaft") : t(`calc.hint.${e.kind}`);
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
export type RefusedAction = "height" | "place" | "face" | "helper" | "transform" | "tower" | "arc";

const REFUSED_TITLE: Record<RefusedAction, string> = {
  height: "refuse.title.height",
  place: "place.refusedTitle",
  face: "refuse.title.face",
  helper: "refuse.title.helper",
  transform: "refuse.title.transform",
  tower: "refuse.title.tower",
  arc: "refuse.title.arc",
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
  // A tower's problem ends with its code (`edit::towers::set_tower`, `tower_preview`).
  const tower = d.match(/\((tower_[a-z_]+)\)\s*$/);
  if (tower && TOWER_CODES.has(tower[1])) {
    // A curve too large names its tier and the largest curve it takes (core `Tower::problems`).
    const most = tower[1] === "tower_bulge" ? d.match(/tier (\d+) takes a curve of at most ([\d.]+) m/) : null;
    if (most) return t("prob.tower_bulgeMax", { n: most[1], max: new Intl.NumberFormat(numberLocale(), { useGrouping: false, maximumFractionDigits: 3 }).format(+most[2]) });
    return t(`prob.${tower[1]}`);
  }
  const arc = arcReason(d);
  if (arc) return arc;
  const range = d.match(/out of range \((-?[\d.]+) to (-?[\d.]+) m/);
  if (range) {
    const f = (x: string) => new Intl.NumberFormat(numberLocale(), { useGrouping: false, maximumFractionDigits: 3 }).format(+x);
    return t("num.range", { min: f(range[1]), max: f(range[2]) });
  }
  return t("refuse.other");
}

/** A refusal as a dialog shows it: the reason, and the core's code and text under it when no reason of ours matched. */
export interface DialogRefusal {
  text: string;
  /** `code: detail` of the core, for a reason we have no text for (else null). */
  detail: string | null;
}

/**
 * A dialog's view of a failed core call: for a refusal (`code: "refused"`)
 * the reason in the user's language, with the core's code and detail when
 * the reason is the general one, so the user has something to report
 * (audit ERR-004). Null for any other error: that goes to `notifyError`.
 */
export function dialogRefusal(e: unknown): DialogRefusal | null {
  const err = toAppError(e);
  if (err.code !== "refused") return null;
  const text = refusalReason(err.detail);
  // A tower's reason is a fragment written to follow "Věž 1:"; in a dialog it starts the sentence.
  return { text: text.charAt(0).toLocaleUpperCase() + text.slice(1), detail: text === t("refuse.other") ? `${err.code}: ${err.detail ?? ""}` : null };
}

/**
 * A tower dialog's view of a failed core call (the preset buttons and OK):
 * a refusal as `dialogRefusal`; also a tower problem the core answers with
 * another code (`tower_preset` answers `out_of_range` with a `(tower_…)`
 * code, review ERR-004), and a job over its counts (`too_many`: the tiers of
 * all towers, ARCH-002) with the core's text under it, so the reason stays
 * beside OK and is not a "could not calculate the roof" toast. Null for a
 * failure of the core.
 */
export function towerRefusal(e: unknown): DialogRefusal | null {
  const r = dialogRefusal(e);
  if (r) return r;
  const err = toAppError(e);
  if ((err.code === "out_of_range" || err.code === "too_many") && /\((tower_[a-z_]+)\)\s*$/.test(err.detail ?? "")) return dialogRefusal({ ...err, code: "refused" });
  if (err.code === "too_many") return { text: describeError(err).message, detail: `${err.code}: ${err.detail ?? ""}` };
  return null;
}

const TOWER_CODES = new Set(["tower_limit", "tower_missing", "tower_not_calculated", "tower_nan", "tower_size", "tower_sides", "tower_corner_radius", "tower_base", "tower_no_tiers", "tower_tiers", "tower_tier_size", "tower_flat_bulge", "tower_bulge", "tower_overhang", "tower_tiers_limit", "tower_label", "tower_tier_pitch"]);

/** The text of each arc refusal code of the core (`edit::arcs::ARC_CODES`). */
const ARC_KEYS = new Map<string, string>([
  ["arc_no_wall", "arc.err.noWall"],
  ["arc_no_corner", "arc.err.noCorner"],
  ["arc_no_part", "arc.err.noPart"],
  ["arc_segments", "arc.err.segments"],
  ["arc_too_short", "arc.err.tooShort"],
  ["arc_flat", "arc.err.flat"],
  ["arc_bulge", "arc.err.bulge"],
  ["arc_radius_min", "arc.err.radiusMin"],
  ["arc_radius_max", "arc.err.radiusMax"],
  ["arc_straight", "arc.err.straight"],
  ["arc_no_length", "arc.err.noLength"],
  ["arc_touches", "arc.err.touches"],
  ["arc_invalid", "arc.err.invalid"],
  ["arc_none", "arc.err.none"],
]);

/**
 * Why the core refused a rounded corner or a bent wall (`edit::arcs`,
 * `commands::arc_edit`), in the user's language; null for another reason.
 */
export function arcReason(detail: string): string | null {
  const f = (x: string) => new Intl.NumberFormat(numberLocale(), { useGrouping: false, maximumFractionDigits: 3 }).format(+x);
  // Every refusal of the core ends with its code (`edit::arcs::ARC_CODES`): the text is
  // chosen by the code; the figures it names are read from the sentence (ARCH-005).
  const code = detail.match(/\((arc_[a-z_]+)\)\s*$/)?.[1];
  const key = code ? ARC_KEYS.get(code) : undefined;
  if (code && key) {
    const n = (re: RegExp) => {
      const v = detail.match(re)?.[1];
      return v === undefined ? "—" : f(v);
    };
    return t(key, {
      max: code === "arc_segments" ? n(/2 to (\d+) walls/) : code === "arc_bulge" ? n(/half the wall \(([\d.]+) m\)/) : n(/at most ([\d.]+) m/),
      min: n(/shorter than ([\d.]+) m/),
    });
  }
  // Without a code (an older core): by the sentence.
  let m = detail.match(/an arc has 2 to (\d+) walls/);
  if (m) return t("arc.err.segments", { max: m[1] });
  m = detail.match(/the bend must be at least [\d.]+ m and at most half the wall \(([\d.]+) m\)/);
  if (m) return t("arc.err.bulge", { max: f(m[1]) });
  if (/the radius must be at least/.test(detail)) return t("arc.err.radiusMin");
  m = detail.match(/the radius is too large for the walls at the corner \(at most ([\d.]+) m\)/);
  if (m) return t("arc.err.radiusMax", { max: f(m[1]) });
  if (/the walls at the corner run straight on/.test(detail)) return t("arc.err.straight");
  m = detail.match(/the arc's walls would be shorter than ([\d.]+) m/);
  if (m) return t("arc.err.tooShort", { min: f(m[1]) });
  if (/(the wall|a wall at the corner) has no length/.test(detail)) return t("arc.err.noLength");
  if (/the arc would make the outline invalid/.test(detail)) return t("arc.err.invalid");
  return null;
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
