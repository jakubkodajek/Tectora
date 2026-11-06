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
});
