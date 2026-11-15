// Stepping through the walls of all buildings, like the previous / next
// buttons of the wall bar, also by keyboard: PageDown / PageUp and Alt+→ /
// Alt+← anywhere in the editor, PageDown / PageUp and Shift+Enter inside a
// wall bar field after it has stored its value (review UX-006).

import type { Project, Selection } from "./types";

/** The wall `d` steps from `selection` over the walls of all buildings, wrapping around; null when there are none. */
export function stepWall(project: Pick<Project, "parts">, selection: Selection, d: number): Selection {
  const all: { part: number; edge: number }[] = project.parts.flatMap((p, pi) => p.edges.map((_, ei) => ({ part: pi, edge: ei })));
  if (all.length === 0) return null;
  const cur = selection ? all.findIndex((s) => s.part === selection.part && s.edge === selection.edge) : -1;
  // Nothing selected yet: the first step forward is the first wall, back the last.
  if (cur < 0) return all[d > 0 ? 0 : all.length - 1];
  return all[(((cur + d) % all.length) + all.length) % all.length];
}

/**
 * The step a key asks for (+1 next wall, -1 previous), or 0. `inField`: the
 * key was pressed in a text field, where Alt+arrows move the caret by word
 * (macOS) and stay with the field; there Shift+Enter goes on to the next wall.
 */
export function wallStepKey(e: Pick<KeyboardEvent, "key" | "altKey" | "ctrlKey" | "metaKey" | "shiftKey">, inField: boolean): number {
  if (e.ctrlKey || e.metaKey) return 0;
  if (e.key === "PageDown") return 1;
  if (e.key === "PageUp") return -1;
  if (inField) return e.key === "Enter" && e.shiftKey && !e.altKey ? 1 : 0;
  if (e.altKey && !e.shiftKey && e.key === "ArrowRight") return 1;
  if (e.altKey && !e.shiftKey && e.key === "ArrowLeft") return -1;
  return 0;
}
