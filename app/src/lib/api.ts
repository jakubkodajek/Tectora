import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  ActivityItem,
  CalcResult,
  ExampleInfo,
  LiveStatus,
  Project,
  ProjectSummary,
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
  listProjects: () => invoke<ProjectSummary[]>("list_projects"),
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
  importDad: (path: string) => invoke<string>("import_dad", { path }),
  calc: (project: Project) => invoke<CalcResult>("calc", { project }),
};

/** Fires with the ids of project files that changed on disk. */
export function onProjectsChanged(cb: (ids: string[]) => void): Promise<UnlistenFn> {
  return listen<string[]>("projects-changed", (e) => cb(e.payload));
}

/** Fires when an AI agent reports what it is doing. */
export function onLiveStatus(cb: (s: LiveStatus) => void): Promise<UnlistenFn> {
  return listen<LiveStatus>("live-status", (e) => cb(e.payload));
}
