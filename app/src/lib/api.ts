import { Channel, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { AppError } from "./errors";
import type { HostSlope } from "./dormers";
import type {
  AiAssistant,
  AiAssistantId,
  ActivityItem,
  AppSettingsView,
  AvailableUpdate,
  Attachment,
  CalcBreakdown,
  CalcResult,
  ExampleInfo,
  JobInfo,
  Library,
  LiveStatus,
  Project,
  ProjectSummary,
  ResultSnapshot,
  ReviewStatus,
  RoofPart,
  StandardKind,
  UpdateProgress,
} from "./types";

/** What a checked save did (`Store::save_checked`). */
export type CheckedSave = { status: "saved"; id: string } | { status: "conflict"; current: Project };

export interface StandardParams {
  kind: StandardKind;
  length: number;
  width: number;
  pitchDeg: number;
  eaveHeight: number;
  overhang: number;
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
  saveProject: (id: string | null, project: Project) => invoke<string>("save_project", { id, project }),
  /** Saves only if the file still holds `base` (the version last read or
      written); otherwise nothing is written and the current file comes back. */
  saveProjectChecked: (id: string, project: Project, base: Project) =>
    invoke<CheckedSave>("save_project_checked", { id, project, base }),
  /** The project in the store's canonical form, as load_project returns it after a save. */
  normalizeProject: (project: Project) => invoke<Project>("normalize_project", { project }),
  deleteProject: (id: string) => invoke<void>("delete_project", { id }),
  /** Copies a job as a new draft; `job` starts a new job from a past roof. */
  duplicateProject: (id: string, name: string, job?: Pick<JobInfo, "number" | "customer" | "address">, withFiles = true) =>
    invoke<string>("duplicate_project", { id, name, job: job ? { ...job, status: "draft" } : null, withFiles }),
  projectActivity: (id: string) => invoke<ActivityItem[]>("project_activity", { id }),
  revertLastChange: (id: string) => invoke<Project>("revert_last_change", { id }),
  revertChanges: (id: string, tsMs: number) => invoke<Project>("revert_changes", { id, tsMs }),
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
  /** Every wall of part `part` at `eaveHeight`, with its breaks and the objects on it (the MCP tool set_part_height). */
  setPartHeight: (project: Project, part: number, eaveHeight: number) =>
    invoke<Project>("set_part_height", { project, part, eaveHeight }),
  /** Hand-drawn face `index` turned into a building (the MCP tool face_to_part). */
  faceToPart: (project: Project, index: number, name: string, pitchDeg: number, eaveHeight: number, overhang: number) =>
    invoke<Project>("face_to_part", { project, index, name, pitchDeg, eaveHeight, overhang }),
  /** The first roof window larger than what is left of its face, or null. */
  checkWindows: (project: Project) =>
    invoke<{ index: number; area: number; face_area: number } | null>("check_windows", { project }),
  /** The roof under dormer `index` at the middle of its front (height, rise per metre into the roof), or null off it. */
  dormerHost: (project: Project, index: number) => invoke<HostSlope | null>("dormer_host", { project, index }),
  getSettings: () => invoke<AppSettingsView>("get_settings"),
  updateSettings: (patch: Record<string, unknown>) => invoke<AppSettingsView>("update_settings", { patch }),
  answerFolderRequest: (allow: boolean) => invoke<AppSettingsView>("answer_folder_request", { allow }),
  openFolder: (which: "projects" | "drawings" | "settings") => invoke<void>("open_folder", { which }),
  listAttachments: (id: string) => invoke<Attachment[]>("list_attachments", { id }),
  attachFiles: (id: string, paths: string[]) => invoke<string[]>("attach_files", { id, paths }),
  removeAttachment: (id: string, name: string) => invoke<void>("remove_attachment", { id, name }),
  /** False: the file's type does not open directly; its folder was shown. */
  openAttachment: (id: string, name: string) => invoke<boolean>("open_attachment", { id, name }),
  exportJob: (id: string, path: string) => invoke<void>("export_job", { id, path }),
  importJob: (path: string) => invoke<string>("import_job", { path }),
  /** The price-sheet text list and the plan sheet (SVG) for the export dialog. */
  exportPreview: (project: Project, title: string, stamp: string) =>
    invoke<{ list: string; svg: string }>("export_preview", { project, title, stamp }),
  /** Writes the plan sheet as PDF or SVG (by the path's extension). */
  exportPlanFile: (project: Project, stamp: string, path: string) =>
    invoke<void>("export_plan_file", { project, stamp, path }),
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

/** Fires when an AI agent reports what it is doing. */
export function onLiveStatus(cb: (s: LiveStatus) => void): Promise<UnlistenFn> {
  return listen<LiveStatus>("live-status", (e) => cb(e.payload));
}
