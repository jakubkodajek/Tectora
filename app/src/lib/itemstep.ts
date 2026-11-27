// Stepping through dormers, chimneys, windows, roof openings and hand-drawn
// lines or faces by keyboard, as the wall bar steps through walls
// (wallstep.ts): PageDown / PageUp anywhere in the editor while the object's
// bar is shown, and inside a field of the bar after the field has stored
// its value.

import { tick } from "svelte";
import { gatedKeys } from "./dragsession";

/** The index `d` steps from `index` over `n` items, wrapping around; null when there are none. */
export function stepIndex(index: number | null, n: number, d: number): number | null {
  if (!(n > 0)) return null;
  if (index === null || !(index >= 0 && index < n)) return d > 0 ? 0 : n - 1;
  return (((index + d) % n) + n) % n;
}

/** +1 for PageDown, -1 for PageUp, 0 for any other key or with a modifier. */
export function itemStepKey(e: Pick<KeyboardEvent, "key" | "altKey" | "ctrlKey" | "metaKey" | "shiftKey">): number {
  if (e.ctrlKey || e.metaKey || e.altKey || e.shiftKey) return 0;
  if (e.key === "PageDown") return 1;
  if (e.key === "PageUp") return -1;
  return 0;
}

/**
 * Whether a bar takes a step key. `inBar`: the key was pressed in the bar;
 * `inField`: in a text field or a list box (anywhere); `dialogOpen`: a
 * dialog is open; `barInDialog`: the bar is part of that dialog.
 */
export function barTakesStep(inBar: boolean, inField: boolean, dialogOpen: boolean, barInDialog: boolean): boolean {
  // A dialog over the editor keeps its keys; a bar behind it does not step.
  if (dialogOpen && !barInDialog) return false;
  // A key in the bar always steps; elsewhere only outside a text field
  // (and, for a bar in a dialog, only inside that dialog: see `inBar`).
  if (inBar) return true;
  return !inField && !dialogOpen;
}

function isField(el: Element | null): el is HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement {
  return el instanceof HTMLInputElement || el instanceof HTMLSelectElement || el instanceof HTMLTextAreaElement;
}

/**
 * Svelte action for an object bar: PageDown / PageUp call `step(+1 | -1)`.
 * In a field of the bar the field first stores its value (blur), and after
 * the step the same field of the next object takes the focus again.
 */
export function pageStep(node: HTMLElement, step: (d: number) => void) {
  let current = step;
  // Not when the editor's refusal took the key, nor while a writer of the
  // job is out (a drag, Revert, the offer put-back; round 12, fix design 1).
  const onkey = gatedKeys(async (e: KeyboardEvent) => {
    const d = itemStepKey(e);
    if (!d) return;
    const target = e.target instanceof Element ? e.target : null;
    const dialog = document.querySelector('[role="dialog"]');
    const inBar = !!target && node.contains(target);
    if (!barTakesStep(inBar, isField(target), !!dialog, !!dialog && dialog.contains(node))) return;
    e.preventDefault();
    const fields = [...node.querySelectorAll<HTMLElement>("input, select")];
    const at = inBar && isField(target) ? fields.indexOf(target as HTMLElement) : -1;
    // Blurring commits the typed value before the selection changes.
    if (at >= 0) (target as HTMLElement).blur();
    current(d);
    if (at < 0) return;
    await tick();
    const again = [...node.querySelectorAll<HTMLElement>("input, select")][at];
    if (again && !(again as HTMLInputElement).disabled) {
      again.focus();
      if (again instanceof HTMLInputElement) again.select();
    }
  });
  window.addEventListener("keydown", onkey);
  return {
    update(next: (d: number) => void) {
      current = next;
    },
    destroy() {
      window.removeEventListener("keydown", onkey);
    },
  };
}
