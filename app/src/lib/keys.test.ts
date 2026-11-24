import { describe, expect, it } from "vitest";
import { deleteKeyHint, drawKeyHint, editorTakesUndo, isDrawKey, isMacPlatform, shortcut, undoKey, zoomKey, draftKey } from "./keys";
import { i18n } from "./i18n.svelte";

const key = (key: string, mods: Partial<{ ctrlKey: boolean; metaKey: boolean; shiftKey: boolean; altKey: boolean }> = {}) => ({
  key,
  ctrlKey: false,
  metaKey: false,
  shiftKey: false,
  altKey: false,
  ...mods,
});

const field = (tagName: string, attrs: string[] = []) => ({ tagName, hasAttribute: (n: string) => attrs.includes(n) });

describe("platform shortcut hints (UX-011)", () => {
  it("tells a Mac from a PC", () => {
    expect(isMacPlatform({ platform: "MacIntel", userAgent: "" })).toBe(true);
    expect(isMacPlatform({ platform: "Win32", userAgent: "" })).toBe(false);
  });

  it("writes ⌘ on a Mac and Ctrl elsewhere", () => {
    expect(shortcut("Ctrl+D", true)).toBe("⌘D");
    expect(shortcut("Ctrl+Shift+Z", true)).toBe("⇧⌘Z");
    expect(shortcut("Ctrl+D", false)).toBe("Ctrl+D");
  });

  it("names the delete key as the keyboard does", () => {
    expect(deleteKeyHint(true)).toBe("⌫");
    i18n.locale = "de";
    expect(deleteKeyHint(false)).toBe("Entf");
    i18n.locale = "cs";
    expect(deleteKeyHint(false)).not.toBe("⌫");
  });

  it("gives a Mac a draw shortcut its keyboard has", () => {
    expect(drawKeyHint(true)).not.toContain("Insert");
    expect(drawKeyHint(false)).toContain("Insert");
    expect(isDrawKey(key("Insert"))).toBe(true);
    expect(isDrawKey(key("b", { metaKey: true }))).toBe(true);
    expect(isDrawKey(key("B", { ctrlKey: true }))).toBe(true);
    expect(isDrawKey(key("b"))).toBe(false);
    expect(isDrawKey(key("b", { ctrlKey: true, shiftKey: true }))).toBe(false);
  });
});

describe("undo keys in a field (UX-001)", () => {
  it("reads undo and redo on both platforms", () => {
    expect(undoKey(key("z", { ctrlKey: true }))).toBe("undo");
    expect(undoKey(key("z", { metaKey: true }))).toBe("undo");
    expect(undoKey(key("Z", { metaKey: true, shiftKey: true }))).toBe("redo");
    expect(undoKey(key("y", { ctrlKey: true }))).toBe("redo");
    expect(undoKey(key("z"))).toBeNull();
  });

  it("undoes the stored change from a number field with nothing typed", () => {
    expect(editorTakesUndo(field("INPUT", ["data-field"]), key("z", { ctrlKey: true }))).toBe(true);
    expect(editorTakesUndo(field("INPUT", ["data-field"]), key("z", { metaKey: true, shiftKey: true }))).toBe(true);
  });

  it("leaves typed text to the browser's own undo", () => {
    expect(editorTakesUndo(field("INPUT", ["data-field", "data-typing"]), key("z", { ctrlKey: true }))).toBe(false);
    // Text fields (names, notes) keep their own undo.
    expect(editorTakesUndo(field("INPUT"), key("z", { ctrlKey: true }))).toBe(false);
    expect(editorTakesUndo(field("TEXTAREA"), key("z", { ctrlKey: true }))).toBe(false);
  });

  it("takes only undo and redo keys, and every key outside a field", () => {
    expect(editorTakesUndo(field("INPUT", ["data-field"]), key("a", { ctrlKey: true }))).toBe(false);
    expect(editorTakesUndo(field("DIV"), key("z", { ctrlKey: true }))).toBe(true);
  });
});

describe("zoom keys (UX-805)", () => {
  it("+ and = zoom in, - zooms out, as the plan hint says", () => {
    expect(zoomKey(key("+"))).toBe(1.25);
    expect(zoomKey(key("+", { shiftKey: true }))).toBe(1.25);
    expect(zoomKey(key("="))).toBe(1.25);
    expect(zoomKey(key("-"))).toBe(0.8);
  });
  it("takes no key with Ctrl, Cmd or Alt (the browser's own zoom) and no other key", () => {
    expect(zoomKey(key("+", { ctrlKey: true }))).toBeNull();
    expect(zoomKey(key("-", { metaKey: true }))).toBeNull();
    expect(zoomKey(key("-", { altKey: true }))).toBeNull();
    expect(zoomKey(key("a"))).toBeNull();
    expect(zoomKey(key("_"))).toBeNull();
  });
});

describe("keys while corners are placed (audit 2026-11-24 UX-1902)", () => {
  const k = (key: string, mods: Partial<{ ctrlKey: boolean; metaKey: boolean; shiftKey: boolean; altKey: boolean }> = {}) => ({
    key,
    ctrlKey: false,
    metaKey: false,
    shiftKey: false,
    altKey: false,
    ...mods,
  });
  it("Ctrl+Z / ⌘Z removes the last corner of a building or a face, Redo does nothing", () => {
    expect(draftKey(k("z", { ctrlKey: true }), "walls", 2)).toBe("drop");
    expect(draftKey(k("z", { metaKey: true }), "manual", 1)).toBe("drop");
    expect(draftKey(k("z", { ctrlKey: true, shiftKey: true }), "walls", 2)).toBe("ignore");
    expect(draftKey(k("y", { ctrlKey: true }), "manual", 2)).toBe("ignore");
  });
  it("Backspace removes a face's or line's last corner; the draw bar has its own for walls", () => {
    expect(draftKey(k("Backspace"), "manual", 3)).toBe("drop");
    expect(draftKey(k("Backspace"), "walls", 3)).toBeNull();
  });
  it("with no corner placed, Ctrl+Z, Redo and Backspace do nothing; Delete never acts (UX-2002, owner decision 6)", () => {
    expect(draftKey(k("z", { ctrlKey: true }), "walls", 0)).toBe("ignore");
    expect(draftKey(k("z", { metaKey: true }), "manual", 0)).toBe("ignore");
    expect(draftKey(k("z", { ctrlKey: true, shiftKey: true }), "walls", 0)).toBe("ignore");
    expect(draftKey(k("y", { ctrlKey: true }), "manual", 0)).toBe("ignore");
    expect(draftKey(k("Backspace"), "manual", 0)).toBe("ignore");
    for (const n of [0, 2]) {
      expect(draftKey(k("Delete"), "manual", n)).toBe("ignore");
      expect(draftKey(k("Delete"), "walls", n)).toBe("ignore");
    }
  });
  it("leaves keys alone when no corners are being placed, and other keys always", () => {
    expect(draftKey(k("z", { ctrlKey: true }), null, 3)).toBeNull();
    expect(draftKey(k("Delete"), null, 0)).toBeNull();
    expect(draftKey(k("Backspace"), null, 0)).toBeNull();
    expect(draftKey(k("a"), "manual", 3)).toBeNull();
    expect(draftKey(k("a"), "walls", 0)).toBeNull();
  });
});
