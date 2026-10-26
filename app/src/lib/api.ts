import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { CalcResult, ExampleInfo, Project, ProjectSummary } from "./types";

export const api = {
  projectsDir: () => invoke<string>("projects_dir"),
  listProjects: () => invoke<ProjectSummary[]>("list_projects"),
  loadProject: (id: string) => invoke<Project>("load_project", { id }),
  saveProject: (id: string | null, project: Project) => invoke<string>("save_project", { id, project }),
  deleteProject: (id: string) => invoke<void>("delete_project", { id }),
  duplicateProject: (id: string) => invoke<string>("duplicate_project", { id }),
  listExamples: () => invoke<ExampleInfo[]>("list_examples"),
  createFromExample: (exampleId: string) => invoke<string>("create_from_example", { exampleId }),
  newProject: (name: string) => invoke<string>("new_project", { name }),
  importDad: (path: string) => invoke<string>("import_dad", { path }),
  calc: (project: Project) => invoke<CalcResult>("calc", { project }),
};

/** Fires with the ids of project files that changed on disk. */
export function onProjectsChanged(cb: (ids: string[]) => void): Promise<UnlistenFn> {
  return listen<string[]>("projects-changed", (e) => cb(e.payload));
}
