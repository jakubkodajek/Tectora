import { Channel, invoke as tauriInvoke } from "@tauri-apps/api/core";
import type { OutboxEntry, ReportRequest, ReportResult } from "./bugreport";
import { logAction } from "./reportlog";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { AppError } from "./errors";
import type { HostSlope } from "./dormers";
import type { OpeningBox } from "./openings";
import type {
  AiAssistant,
  ArcInput,
  AiAssistantId,
  ActivityItem,
  AppSettingsView,
  AvailableUpdate,
  Attachment,
  Chimney,
  EdgeDef,
  ManualFace,
  Point2,
  CalcBreakdown,
  CalcResult,
  HelperContext,
  HelperPreview,
  HelperRequest,
  PlanMapSpec,
  RoundArcAnswer,
  ExampleInfo,
  FolderRequest,
  JobInfo,
  Library,
  LiveStatus,
  Placement,
  Project,
  ProjectSummary,
  Recalculation,
  RefreshedEntry,
  ResultSnapshot,
  ReviewStatus,
  RoofPart,
  StandardKind,
  TrashEntry,
  UpdateProgress,
} from "./types";

/** What a checked save did (`Store::save_checked`). */
/** The price-sheet list as numbers (`tectora_core::export::ListSummary`). */
export interface ListSummary {
  decimals: number;
  windows: { face: string; width: number; height: number }[];
  faces: { name: string; pitch: number; area: number; deducted: number; rafter: number | null }[];
  wall_area: number | null;
  materials: { material: number; area: number }[];
  lines: { code: string; length: number }[];
}

/** `repaired`: the file could not be read (another program damaged it) and was written over (audit 2026-11-23 ERR-1502). */
export type CheckedSave = { status: "saved"; id: string; repaired?: boolean } | { status: "conflict"; current: Project };
/** What Revert all did (`Store::revert_to_counted`): the job as it is now and how many recorded changes it undid. */
export interface RevertedChanges {
  project: Project;
  /** The changes undone (reverts among them not counted again), as the history line says; 0 when nothing was written. */
  undone: number;
  /** The AI's entries undone, the banner's own rule (`isAi`): compared with the AI steps the banner showed (HUNT-1401). */
  undone_ai: number;
  /** The job file could not be read (another program damaged it) and the revert wrote over it (audit 2026-11-23 ERR-1502). */
  repaired?: boolean;
  /** The revert entry just logged, null when nothing was written: the editor's Undo takes exactly this one back (`takeBackRevert`, UX-1601). */
  entry_ts_ms?: number | null;
}

export interface StandardParams {
  kind: StandardKind;
  length: number;
  width: number;
  pitchDeg: number;
  eaveHeight: number;
  overhang: number;
}

/**
 * `invoke` that notes the command in the report log: its name, whether it
 * worked, the error code and how long it took. Never its arguments.
 */
async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const t0 = performance.now();
  try {
    const r = await tauriInvoke<T>(cmd, args);
    logAction({ kind: "command", name: cmd, ok: true, ms: performance.now() - t0 });
    return r;
  } catch (e) {
    // A bare string is the code itself (open_folder answers "not_writable").
    const code = typeof e === "string" ? e : e && typeof e === "object" && "code" in e ? String((e as { code: unknown }).code) : null;
    logAction({ kind: "command", name: cmd, ok: false, code, ms: performance.now() - t0 });
    throw e;
  }
}

/** The queue of bug reports, as the app answers it. */
export interface BugReportOutbox {
  /** False in a build without a relay: reports can only be saved as files. */
  sendable: boolean;
  counts: { waiting: number; failed: number; local: number };
  reports: OutboxEntry[];
}

export const api = {
  /** A newer version of the app, if the update server offers one. */
  updateCheck: () => invoke<AvailableUpdate | null>("update_check"),
  /** Downloads and installs the update the check found; the app then restarts. */
  updateInstall: (onProgress: (p: UpdateProgress) => void) => {
    const progress = new Channel<UpdateProgress>();
    progress.onmessage = onProgress;
    return invoke<void>("update_install", { progress });
  },
  projectsDir: () => invoke<string>("projects_dir"),
  listProjects: () => invoke<Library>("list_projects"),
  startupWarnings: () => invoke<AppError[]>("startup_warnings"),
  loadProject: (id: string) => invoke<Project>("load_project", { id }),
  /** What reading the job left out of it (a stored result with an impossible
      time, `result_dropped`), to tell when the job opens; empty when nothing was. */
  loadWarnings: (id: string) => invoke<AppError[]>("load_warnings", { id }),
  /** Recalculates the job's stored result where an older engine stored it,
      before the job is loaded (`load_project` no longer does): the change
      log's summary when it did, else null. Never fails; an error is logged
      and the stored result stays. */
  refreshStoredResult: (id: string) => invoke<RefreshedEntry | null>("refresh_stored_result", { id }),
  saveProject: (id: string | null, project: Project) => invoke<string>("save_project", { id, project }),
  /** Saves only if the file still holds `base` (the version last read or
      written); otherwise nothing is written and the current file comes back. */
  saveProjectChecked: (id: string, project: Project, base: Project) =>
    invoke<CheckedSave>("save_project_checked", { id, project, base }),
  /** The project in the store's canonical form, as load_project returns it after a save. */
  normalizeProject: (project: Project) => invoke<Project>("normalize_project", { project }),
  /** Moves the job to the library's trash; the entry brings it back (`restoreDeleted`). */
  deleteProject: (id: string) => invoke<TrashEntry>("delete_project", { id }),
  /** Jobs and files in the library's trash (kept 30 days), newest first. */
  listDeleted: () => invoke<TrashEntry[]>("list_deleted"),
  /** Brings a trash entry back; resolves to the job's id (a new one if the old id was taken). */
  restoreDeleted: (entry: string) => invoke<string>("restore_deleted", { entry }),
  /** Names the example jobs seeded on this first start in the UI's language (example id to name). */
  nameSeededExamples: (names: Record<string, string>) => invoke<number>("name_seeded_examples", { names }),
  /** Copies a job as a new draft; `job` starts a new job from a past roof. */
  duplicateProject: (id: string, name: string, job?: Pick<JobInfo, "number" | "customer" | "address">, withFiles = true) =>
    invoke<string>("duplicate_project", { id, name, job: job ? { ...job, status: "draft" } : null, withFiles }),
  projectActivity: (id: string) => invoke<ActivityItem[]>("project_activity", { id }),
  /** The engine's recalculation the job's stored result is, if it still is one. */
  resultRecalculation: (id: string) => invoke<Recalculation | null>("result_recalculation", { id }),
  /** Puts the offer's own stored result back in place of the recalculation logged at `entryTsMs`, and nothing else. */
  restoreOfferResult: (id: string, entryTsMs: number) => invoke<Project>("restore_offer_result", { id, entryTsMs }),
  /** Undoes the latest change; with `expectTs` only if it is still that entry (`revert_stale` otherwise). */
  /** Answers as Revert all (`Store::revert_last_reported`), with `repaired` (ERR-1502). */
  revertLastChange: (id: string, expectTs?: number) => invoke<RevertedChanges>("revert_last_change", { id, expectTs: expectTs ?? null }),
  /** Revert all: undoes the change at `tsMs` and every later one; `undone_ai` counts the AI entries undone, also ones that came in after the press (ERR-1002, HUNT-1401). `seenTs`: the newest entry the editor has applied (its Undo over an AI's save); the core answers `revert_stale` when a newer one is there (ERR-1704). */
  revertAll: (id: string, tsMs: number, seenTs?: number) => invoke<RevertedChanges>("revert_changes", { id, tsMs, seenTs: seenTs ?? null }),
  /** Revert all of the review the banner showed (`since_ts_ms`): writes the job as the user last settled it (ADR 0007); `revert_stale` when that review is not open any more (ERR-1905). */
  revertReview: (id: string, sinceTsMs: number) => invoke<RevertedChanges>("revert_changes", { id, tsMs: sinceTsMs, seenTs: null, review: true }),
  revertChanges: (id: string, tsMs: number) =>
    invoke<RevertedChanges>("revert_changes", { id, tsMs, seenTs: null }).then((r) => r.project),
  /** Takes back the review revert logged at `revertTs` (Undo / Redo of a Revert), itself logged (UX-1601). `seenTs`: the newest entry the editor has applied; `revert_stale` when the log ends at another one (ARCH-1703, design B). */
  takeBackRevert: (id: string, revertTs: number, seenTs: number) => invoke<RevertedChanges>("take_back_revert", { id, revertTs, seenTs }),
  reviewStatus: (id: string) => invoke<ReviewStatus | null>("review_status", { id }),
  acceptChanges: (id: string, tsMs: number) => invoke<number>("accept_changes", { id, tsMs }),
  liveStatus: () => invoke<LiveStatus | null>("live_status"),
  mcpCommand: () => invoke<{ command: string; args: string[] }>("mcp_command"),
  aiAssistants: () => invoke<AiAssistant[]>("ai_assistants"),
  connectAiAssistant: (id: AiAssistantId) => invoke<"added" | "updated" | "unchanged">("connect_ai_assistant", { id }),
  createStandard: (name: string, p: StandardParams) => invoke<string>("create_standard", { name, ...p }),
  standardPart: (p: StandardParams) => invoke<RoofPart>("standard_part", { ...p }),
  listExamples: () => invoke<ExampleInfo[]>("list_examples"),
  createFromExample: (exampleId: string, name?: string) =>
    invoke<string>("create_from_example", { exampleId, name: name ?? null }),
  calc: (project: Project) => invoke<CalcResult>("calc", { project }),
  calcBreakdown: (project: Project) => invoke<CalcBreakdown>("calc_breakdown", { project }),
  /** Where a dormer, chimney, window or opening sits on its roof face; null off the roof. */
  itemPlacement: (project: Project, kind: string, index: number) =>
    invoke<Placement | null>("item_placement", { project, kind, index }),
  /** Moves a dormer, chimney, window or opening to distances on its roof face (the MCP tool place_item). */
  placeItem: (project: Project, kind: string, index: number, fromEave: number | null, fromLeft: number | null, fromRight: number | null) =>
    invoke<Project>("place_item", { project, kind, index, fromEave, fromLeft, fromRight }),
  /** Building `part` at `height` (top of the walls); its roof and the objects on it move by the change (the MCP tool set_part_height). */
  setPartHeight: (project: Project, part: number, height: number) =>
    invoke<Project>("set_part_height", { project, part, height }),
  /** Hand-drawn face `index` turned into a building (the MCP tool face_to_part). */
  faceToPart: (project: Project, index: number, name: string, pitchDeg: number, eaveHeight: number, overhang: number) =>
    invoke<Project>("face_to_part", { project, index, name, pitchDeg, eaveHeight, overhang }),
  /** The first roof window larger than what is left of its face, or null. */
  checkWindows: (project: Project) =>
    invoke<{ index: number; area: number; face_area: number } | null>("check_windows", { project }),
  /** The roof under dormer `index` at the middle of its front (height, rise per metre into the roof), or null off it. */
  dormerHost: (project: Project, index: number) => invoke<HostSlope | null>("dormer_host", { project, index }),
  /** Whether dormer `index` can be stored as it is; refused with `dormer_pitch`, `dormer_segments` or `dormer_below_roof` (core `check_dormer`). */
  checkDormer: (project: Project, index: number) => invoke<null>("check_dormer", { project, index }),
  /** What the calculation dialog needs to know about wall `edge` of `part`: facing wall, span, parallel walls, roof points (`withPoints`). */
  roofHelperContext: (project: Project, part: number, edge: number, withPoints = false) =>
    invoke<HelperContext>("roof_helper_context", { project, part, edge, withPoints }),
  /**
   * The values a helper works out, nothing changed (the MCP tool roof_helper
   * without apply). Null: the values have no solution (the core refused).
   */
  roofHelperPreview: async (project: Project, part: number, edge: number, request: HelperRequest): Promise<HelperPreview | null> => {
    try {
      const r = await invoke<Omit<HelperPreview, "patches"> & { patches?: Record<string, Partial<EdgeDef>> }>("roof_helper_preview", { project, part, edge, request });
      return { ...r, patches: new Map(Object.entries(r.patches ?? {}).map(([k, v]) => [Number(k), v])) };
    } catch (e) {
      if ((e as { code?: string } | null)?.code === "refused") return null;
      throw e;
    }
  },
  /** The project with a helper's values applied (the MCP tool roof_helper with apply). */
  roofHelperApply: (project: Project, part: number, edge: number, request: HelperRequest) =>
    invoke<Project>("roof_helper_apply", { project, part, edge, request }),
  /** The round-roof dialog's arc for the typed values, the walls' values, DACAD's default arc and the smallest radius. */
  roundArc: (input: ArcInput) => invoke<RoundArcAnswer>("round_arc", { input }),
  /**
   * The corners of a chimney or roof opening stretched and turned so that
   * their box becomes `to` (the MCP tool update_opening's operation); a box
   * the core refuses (size out of range, no area) rejects with code "refused".
   */
  reshapeOpening: (corners: Point2[], to: OpeningBox) =>
    invoke<{ corners: Point2[] }>("item_geometry", {
      request: { op: "reshape_opening", corners, to: { x: to.x, y: to.y, width: to.width, depth: to.depth, rotation_deg: to.rotation } },
    }).then((r) => r.corners),
  /** A rectangular chimney from a corner, size and rotation in degrees. */
  chimneyFromRect: (r: { x: number; y: number; width: number; depth: number; rotation: number; height: number }) =>
    invoke<Chimney>("item_geometry", { request: { op: "chimney_from_rect", x: r.x, y: r.y, width: r.width, depth: r.depth, rotation_deg: r.rotation, height: r.height } }),
  /** The sloped area of a hand-drawn face, m². */
  faceArea: (face: ManualFace) => invoke<{ area: number }>("item_geometry", { request: { op: "face_area", face } }).then((r) => r.area),
  /** Why the core refuses a new hand-drawn face (`face_corners`, `pitch_range`, `face_crosses`: corners out of order), or null (HUNT-1801). */
  checkFace: (face: ManualFace) =>
    invoke<{ refused: "face_corners" | "pitch_range" | "face_crosses" | null }>("item_geometry", { request: { op: "check_face", face } }).then((r) => r.refused),
  /** Part `part` moved, rotated, mirrored or copied with what lies on it (the MCP tool transform_part); the project afterwards and the moved or new parts. */
  transformPart: (project: Project, part: number, map: PlanMapSpec, copies: number) =>
    invoke<{ project: Project; parts: number[] }>("transform_part", { project, part, map, copies }),
  /** Wall `edge` moved outward by `dist` m, its neighbours stretched to meet it; "refused" where a neighbour runs parallel. */
  offsetWall: (project: Project, part: number, edge: number, dist: number) =>
    invoke<Project>("offset_wall", { project, part, edge, dist }),
  /** The corners of a regular polygon, counter-clockwise, the first wall along the x axis. */
  regularPolygon: (centre: Point2, sides: number, side: number, rotationDeg = 0) =>
    invoke<{ corners: Point2[] }>("item_geometry", { request: { op: "regular_polygon", centre, sides, side, rotation_deg: rotationDeg } }).then((r) => r.corners),
  /** The centre of a part's wall outline, the transform dialog's default pivot. */
  partCentre: (part: RoofPart) =>
    invoke<{ centre: Point2 }>("item_geometry", { request: { op: "part_centre", part } }).then((r) => r.centre),
  /** Corner `vertex` dropped onto its neighbour: the part with the corner merged into it, or unchanged. */
  mergeDroppedCorner: (part: RoofPart, vertex: number) =>
    invoke<{ part: RoofPart; merged: boolean }>("item_geometry", { request: { op: "merge_dropped_corner", part, vertex } }),
  /** Wall `edge` set to `length` m with the outline's angles kept (core `edit::corners::set_wall_length`). */
  setWallLength: (part: RoofPart, edge: number, length: number) =>
    invoke<{ part: RoofPart }>("item_geometry", { request: { op: "set_wall_length", part, edge, length } }).then((r) => r.part),
  /** A drawn footprint without repeated corners (closer than 1 cm). */
  withoutRepeats: (points: Point2[]) =>
    invoke<{ points: Point2[] }>("item_geometry", { request: { op: "without_repeats", points } }).then((r) => r.points),
  getSettings: () => invoke<AppSettingsView>("get_settings"),
  updateSettings: (patch: Record<string, unknown>) => invoke<AppSettingsView>("update_settings", { patch }),
  /** `changes`: the request the dialog showed; a request replaced meanwhile is not answered. */
  answerFolderRequest: (changes: FolderRequest["changes"], allow: boolean) =>
    invoke<AppSettingsView>("answer_folder_request", { changes, allow }),
  /** `path`: the request the dialog showed; a request replaced meanwhile is not answered. */
  answerFileRequest: (path: string, allow: boolean) =>
    invoke<AppSettingsView>("answer_file_request", { path, allow }),
  /** The user's answer to an AI settings change: `revert` puts the values before it back. */
  answerAiChange: (id: number, revert: boolean) => invoke<AppSettingsView>("answer_ai_change", { id, revert }),
  openFolder: (which: "projects" | "drawings" | "settings") => invoke<void>("open_folder", { which }),
  listAttachments: (id: string) => invoke<Attachment[]>("list_attachments", { id }),
  attachFiles: (id: string, paths: string[]) => invoke<string[]>("attach_files", { id, paths }),
  removeAttachment: (id: string, name: string) => invoke<void>("remove_attachment", { id, name }),
  /** False: the file's type does not open directly; its folder was shown. */
  openAttachment: (id: string, name: string) => invoke<boolean>("open_attachment", { id, name }),
  exportJob: (id: string, path: string) => invoke<void>("export_job", { id, path }),
  importJob: (path: string) => invoke<string>("import_job", { path }),
  /**
   * The price-sheet text list, the plan sheet (SVG) and the list as numbers
   * (`summary`, shown translated) for the export dialog; `lineNames` label
   * the sheet's line codes in the UI language.
   */
  exportPreview: (project: Project, title: string, stamp: string, lineNames: Record<string, string> = {}) =>
    invoke<{ list: string; svg: string; summary: ListSummary }>("export_preview", { project, title, stamp, lineNames }),
  /** Writes the plan sheet as PDF or SVG (by the path's extension). */
  exportPlanFile: (project: Project, stamp: string, path: string, lineNames: Record<string, string> = {}) =>
    invoke<void>("export_plan_file", { project, stamp, path, lineNames }),
  /** Queues a bug report and sends it, or writes it to `path` (Save as). */
  createBugReport: (request: ReportRequest, path: string | null) => invoke<ReportResult>("create_bug_report", { request, path }),
  /** The default file name of a report, for the Save as dialog. */
  bugReportName: (jobName: string | null, stamp: string) => invoke<string>("bug_report_name", { jobName, stamp }),
  /** Shows a saved report in Finder / Explorer. */
  showBugReport: (path: string) => invoke<void>("show_bug_report", { path }),
  /** The reports that wait to be sent, their counts and whether this build can send. */
  bugReportOutbox: () => invoke<BugReportOutbox>("bug_report_outbox"),
  /** Tries to send everything that waits, now. */
  sendBugReports: () => invoke<{ sent: number; waiting: number; failed: number; last_error?: string }>("send_bug_reports"),
  /** Save as of a queued report; it stays queued. */
  saveQueuedBugReport: (name: string, path: string) => invoke<ReportResult>("save_queued_bug_report", { name, path }),
  /** Removes a queued report that was not sent. */
  discardBugReport: (name: string) => invoke<void>("discard_bug_report", { name }),
  savePng: (path: string, bytes: number[]) => invoke<void>("save_png", { path, bytes }),
  takeSnapshot: (project: Project) => invoke<ResultSnapshot>("take_snapshot", { project }),
  takePendingOpen: () => invoke<string | null>("take_pending_open"),
  /** Files opened from the system (double-click, start-up) that did not open. */
  takeOpenErrors: () => invoke<AppError[]>("take_open_errors"),
};

/** Fires when the settings file changes (e.g. an AI agent changed a setting). */
export function onSettingsChanged(cb: (v: AppSettingsView) => void): Promise<UnlistenFn> {
  return listen<AppSettingsView>("settings-changed", (e) => cb(e.payload));
}

/** Fires when a file opened from the system did not open; `takeOpenErrors` has it. */
export function onOpenFailed(cb: () => void): Promise<UnlistenFn> {
  return listen<AppError>("open-failed", () => cb());
}

/** Fires with the id of a job opened from a file (double-click). */
export function onOpenProject(cb: (id: string) => void): Promise<UnlistenFn> {
  return listen<string>("open-project", (e) => cb(e.payload));
}

/** Fires with the ids of project files that changed on disk. */
export function onProjectsChanged(cb: (ids: string[]) => void): Promise<UnlistenFn> {
  return listen<string[]>("projects-changed", (e) => cb(e.payload));
}

/** Fires with the counts of the bug report outbox when they change (it only refreshes a count, never writes). */
export function onReportOutbox(cb: (c: { waiting: number; failed: number; local: number }) => void): Promise<UnlistenFn> {
  return listen<{ waiting: number; failed: number; local: number }>("report-outbox", (e) => cb(e.payload));
}

/** Fires when an AI agent reports what it is doing. */
export function onLiveStatus(cb: (s: LiveStatus) => void): Promise<UnlistenFn> {
  return listen<LiveStatus>("live-status", (e) => cb(e.payload));
}
