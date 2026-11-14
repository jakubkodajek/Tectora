// What stays selected when the job is replaced under the editor (undo, redo).

import type { Project, Selection } from "./types";

/**
 * The wall selection after the job became `project`: kept while that wall
 * still exists, so an undo of a typo leaves the wall in the bar (review
 * UX-005); null when the part or wall is gone.
 */
export function keptSelection(project: Project | null, sel: Selection): Selection {
  if (!project || !sel) return null;
  return project.parts[sel.part]?.edges[sel.edge] ? { part: sel.part, edge: sel.edge } : null;
}
