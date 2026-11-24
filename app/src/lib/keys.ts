// Keyboard shortcuts as each platform names and presses them. macOS shows
// ⌘ for Ctrl, has no Delete or Insert key on its laptop keyboards (its
// "delete" sends Backspace), so every hint is written per platform and the
// handlers take a key a Mac keyboard has (review UX-011).

import { t } from "./i18n.svelte";

/** Whether the app runs on macOS (or an iPad), from the web view's platform. */
export function isMacPlatform(nav: Pick<Navigator, "platform" | "userAgent"> | undefined = globalThis.navigator): boolean {
  return !!nav && /Mac|iPhone|iPad/.test(nav.platform || nav.userAgent || "");
}

/** A Windows-style shortcut ("Ctrl+Shift+Z") as the platform writes it (⇧⌘Z on a Mac). */
export function shortcut(k: string, mac: boolean = isMacPlatform()): string {
  return mac ? k.replace("Ctrl+Shift+", "⇧⌘").replace("Ctrl+", "⌘") : k;
}

/** The delete key's name: ⌫ on a Mac, the keyboard's label elsewhere (de "Entf"). */
export function deleteKeyHint(mac: boolean = isMacPlatform()): string {
  return mac ? "⌫" : t("key.delete");
}

/**
 * The shortcut that starts a new building outline: Insert on a PC keyboard,
 * Ctrl+B / ⌘B everywhere (a Mac has no Insert key).
 */
export function drawKeyHint(mac: boolean = isMacPlatform()): string {
  return mac ? "⌘B" : "Insert, Ctrl+B";
}

type KeyLike = Pick<KeyboardEvent, "key" | "ctrlKey" | "metaKey" | "shiftKey" | "altKey">;

/** Whether a key press asks for a new building outline (Insert, or Ctrl / ⌘ + B). */
export function isDrawKey(e: KeyLike): boolean {
  if (e.key === "Insert") return !e.ctrlKey && !e.metaKey && !e.altKey;
  return (e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey && e.key.toLowerCase() === "b";
}

/** "undo" or "redo" for Ctrl/⌘+Z, Ctrl/⌘+Shift+Z and Ctrl/⌘+Y; null for any other key. */
export function undoKey(e: KeyLike): "undo" | "redo" | null {
  if (!(e.ctrlKey || e.metaKey) || e.altKey) return null;
  const k = e.key.toLowerCase();
  if (k === "z") return e.shiftKey ? "redo" : "undo";
  if (k === "y" && !e.shiftKey) return "redo";
  return null;
}

/** The parts of a focused element the editor looks at. */
export interface FieldLike {
  tagName: string;
  /** `numeric` marks its fields with `data-field`; `data-typing` while text is not stored yet. */
  hasAttribute(name: string): boolean;
}

/**
 * Whether the editor's own undo / redo takes a key pressed in a field: in a
 * number field that holds no typed text (its value is stored, e.g. right
 * after Enter), Ctrl+Z undoes the stored change instead of only putting the
 * field's old text back while the roof keeps the new value (review UX-001).
 * While there is typing, the browser's own text undo stays.
 */
export function editorTakesUndo(target: FieldLike | null, e: KeyLike): boolean {
  if (!undoKey(e)) return false;
  if (!target) return true;
  const tag = target.tagName;
  if (tag !== "INPUT" && tag !== "TEXTAREA" && tag !== "SELECT") return true;
  return tag === "INPUT" && target.hasAttribute("data-field") && !target.hasAttribute("data-typing");
}

/**
 * The plan's zoom factor for a key, as the toolbar's + / − buttons zoom:
 * `+` (also `=`, the same key without Shift on many layouts) zooms in, `-`
 * out; the number pad's keys give the same `key`. Null with Ctrl, ⌘ or
 * Alt (the browser's own zoom) and for any other key (UX-805).
 */
export function zoomKey(e: KeyLike): 1.25 | 0.8 | null {
  if (e.ctrlKey || e.metaKey || e.altKey) return null;
  if (e.key === "+" || e.key === "=") return 1.25;
  if (e.key === "-") return 0.8;
  return null;
}

/**
 * What a key does while corners are being placed (`draft`: a new
 * building's walls, or a hand-drawn face's or line's corners, with
 * `points` placed): Ctrl/⌘+Z removes the last corner, as the bar's button
 * does, and touches nothing else; Redo does nothing (`ignore`); Backspace
 * removes a face's or line's last corner (the draw bar takes it for walls).
 * With no corner placed these keys do nothing (`ignore`), and Delete never
 * acts while corners are placed: one press more than there are corners
 * must not undo an earlier roof edit or delete a selected wall out of
 * sight (audit 2026-11-24 UX-1902, owner decision 3; round 21 UX-2002,
 * owner decision 6). `null`: the key is not theirs.
 */
export function draftKey(e: KeyLike, draft: "walls" | "manual" | null, points: number): "drop" | "ignore" | null {
  if (!draft) return null;
  const u = undoKey(e);
  if (u) return u === "undo" && points > 0 ? "drop" : "ignore";
  const plain = !e.ctrlKey && !e.metaKey && !e.altKey;
  if (draft === "manual" && e.key === "Backspace" && plain) return points > 0 ? "drop" : "ignore";
  if (e.key === "Delete") return "ignore";
  return null;
}
