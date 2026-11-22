/// <reference types="vite/client" />
// Tauri's dialog plugin replaces window.confirm with an asynchronous version:
// it returns a Promise, which is always truthy. A confirm() that is not
// awaited therefore never cancels (a removed file stays removed even after
// "Cancel"). This guards every call in the UI sources.
import { describe, expect, it } from "vitest";

const files = import.meta.glob(["../**/*.svelte", "../**/*.ts", "!../**/*.test.ts", "!../dev/**"], {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

describe("dialogs", () => {
  it("reads the UI sources", () => {
    expect(Object.keys(files).some((f) => f.endsWith("Editor.svelte"))).toBe(true);
  });

  it("every confirm() is awaited", () => {
    const bad: string[] = [];
    for (const [file, text] of Object.entries(files)) {
      text.split("\n").forEach((line: string, i: number) => {
        if (/(^|[^.\w])confirm\(/.test(line) && !/await\s+confirm\(/.test(line) && !line.trim().startsWith("//")) {
          bad.push(`${file}:${i + 1}: ${line.trim()}`);
        }
      });
    }
    expect(bad).toEqual([]);
  });

  // UI-503 / UI-604: a focused button that becomes `disabled` while its
  // action waits drops the keyboard focus to the page. Buttons that wait
  // stay enabled, say `aria-disabled` / `aria-busy` and ignore a second press.
  it("the request dialogs, the AI settings card and the AI panel never disable the pressed button while they wait", () => {
    for (const name of ["FileRequest.svelte", "FolderRequest.svelte", "AiSettingsChange.svelte", "AiPanel.svelte"]) {
      const [file, text] = Object.entries(files).find(([f]) => f.endsWith(`/${name}`))!;
      expect(text, file).not.toMatch(/\sdisabled=\{busy\}/);
      const waiting = text.split("\n").filter((l) => /<button[^>]*aria-disabled=\{busy\}/.test(l));
      expect(waiting.length, file).toBeGreaterThan(0);
      for (const l of waiting) {
        expect(l, file).toContain("aria-busy={busy}");
        // The press is ignored while busy: by the handler's own guard (answer) or at the call.
        expect(/onclick=\{\(\) => answer\(/.test(l) || /onclick=\{\(\) => !busy && /.test(l), `${file}: ${l.trim()}`).toBe(true);
      }
      if (/onclick=\{\(\) => answer\(/.test(text)) expect(text, file).toMatch(/async function answer\([^)]*\) \{\n(\s*\/\/[^\n]*\n)*\s*if \(busy\) return;/);
    }
  });
});
