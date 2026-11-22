import { describe, expect, it } from "vitest";
import { deleteKeyHint, drawKeyHint, editorTakesUndo, isDrawKey, isMacPlatform, shortcut, undoKey, zoomKey } from "./keys";
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
