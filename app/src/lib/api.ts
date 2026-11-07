import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { AppError } from "./errors";
import type {
  ActivityItem,
  AppSettingsView,
  Attachment,
  CalcBreakdown,
  CalcResult,
  ExampleInfo,
  Library,
  LiveStatus,
  Project,
  ProjectSummary,
  ResultSnapshot,
  RoofPart,
  StandardKind,
} from "./types";

export interface StandardParams {
  kind: StandardKind;
  length: number;
  width: number;
  pitchDeg: number;
  eaveHeight: number;
  overhang: number;
}

export const api = {
  projectsDir: () => invoke<string>("projects_dir"),
  listProjects: () => invoke<Library>("list_projects"),
  startupWarnings: () => invoke<AppError[]>("startup_warnings"),
  loadProject: (id: string) => invoke<Project>("load_project", { id }),
  saveProject: (id: string | null, project: Project) => invoke<string>("save_project", { id, project }),
  deleteProject: (id: string) => invoke<void>("delete_project", { id }),
  duplicateProject: (id: string, name: string) => invoke<string>("duplicate_project", { id, name }),
  projectActivity: (id: string) => invoke<ActivityItem[]>("project_activity", { id }),
  revertLastChange: (id: string) => invoke<Project>("revert_last_change", { id }),
  revertChanges: (id: string, tsMs: number) => invoke<Project>("revert_changes", { id, tsMs }),
  liveStatus: () => invoke<LiveStatus | null>("live_status"),
  mcpCommand: () => invoke<{ command: string; args: string[] }>("mcp_command"),
  createStandard: (name: string, p: StandardParams) => invoke<string>("create_standard", { name, ...p }),
  standardPart: (p: StandardParams) => invoke<RoofPart>("standard_part", { ...p }),
  listExamples: () => invoke<ExampleInfo[]>("list_examples"),
  createFromExample: (exampleId: string, name?: string) =>
    invoke<string>("create_from_example", { exampleId, name: name ?? null }),
  calc: (project: Project) => invoke<CalcResult>("calc", { project }),
  calcBreakdown: (project: Project) => invoke<CalcBreakdown>("calc_breakdown", { project }),
  getSettings: () => invoke<AppSettingsView>("get_settings"),
  updateSettings: (patch: Record<string, unknown>) => invoke<AppSettingsView>("update_settings", { patch }),
  openFolder: (which: "projects" | "drawings" | "settings") => invoke<void>("open_folder", { which }),
  listAttachments: (id: string) => invoke<Attachment[]>("list_attachments", { id }),
  attachFiles: (id: string, paths: string[]) => invoke<string[]>("attach_files", { id, paths }),
  removeAttachment: (id: string, name: string) => invoke<void>("remove_attachment", { id, name }),
  openAttachment: (id: string, name: string) => invoke<void>("open_attachment", { id, name }),
  exportJob: (id: string, path: string) => invoke<void>("export_job", { id, path }),
  importJob: (path: string) => invoke<string>("import_job", { path }),
  takeSnapshot: (project: Project) => invoke<ResultSnapshot>("take_snapshot", { project }),
  takePendingOpen: () => invoke<string | null>("take_pending_open"),
};

/** Fires when the settings file changes (e.g. an AI agent changed a setting). */
export function onSettingsChanged(cb: (v: AppSettingsView) => void): Promise<UnlistenFn> {
  return listen<AppSettingsView>("settings-changed", (e) => cb(e.payload));
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
