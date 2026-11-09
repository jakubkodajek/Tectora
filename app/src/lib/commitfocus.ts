// A press outside the focused text field commits that field first.
//
// Fields store their value on `change`, which the browser sends on blur. A
// press on the plan selects another wall, line or object on `pointerdown`,
// before the field loses focus, so the value went to the newly selected
// object or was lost; and WebKit (macOS) does not move the focus to a button
// that is clicked at all. Blurring the field in the capture phase, before
// any handler of the press runs, sends `change` while the old selection is
// still current. Number fields (lib/numeric.svelte.ts) do the same for
// their own ▲▼ buttons.

/** Blurs the focused text field when a press lands outside it (and outside its own controls). */
export function commitBeforePress(target: Node | null, active: Element | null): boolean {
  if (!(active instanceof HTMLInputElement || active instanceof HTMLTextAreaElement)) return false;
  if (active instanceof HTMLInputElement && !["text", "search", "email", "url", "tel", "password", ""].includes(active.type)) return false;
  if (target && (active.contains(target) || active.parentElement?.querySelector(".num-spin")?.contains(target))) return false;
  active.blur();
  return true;
}

export function installCommitBeforePress(win: Window = window): () => void {
  const onPress = (e: Event) => commitBeforePress(e.target as Node | null, win.document.activeElement);
  win.addEventListener("pointerdown", onPress, true);
  return () => win.removeEventListener("pointerdown", onPress, true);
}

/** Commits whatever field has the focus (before leaving the editor or closing the window). */
export function commitFocused(doc: Document = document) {
  const a = doc.activeElement;
  if (a instanceof HTMLInputElement || a instanceof HTMLTextAreaElement) a.blur();
}
