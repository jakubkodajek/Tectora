// The price-sheet text list ("text for Excel") as the core writes it
// (tectora_core::export, the MCP tool export_list). The Export dialog and
// the Results tab's "Copy for Excel" both get it here, from the same
// command with the same title and stamp, so the two copies are byte for
// byte the same text (UX-004).
import { api } from "./api";
import type { Project } from "./types";

/** Local time as `dd.mm.yyyy hh:mm`, the stamp the list and sheet carry. */
export function stampNow(): string {
  const d = new Date();
  const p = (n: number) => String(n).padStart(2, "0");
  return `${p(d.getDate())}.${p(d.getMonth() + 1)}.${d.getFullYear()} ${p(d.getHours())}:${p(d.getMinutes())}`;
}

/** The title the list and the plan sheet carry: the job's name, else its id. */
export const exportTitle = (project: Project, id: string) => project.name || id;

/** The list and the plan sheet (SVG) for `project`, as the Export dialog shows them. */
export function exportPreview(project: Project, title: string, stamp = stampNow()) {
  return api.exportPreview(project, title, stamp);
}
