import { describe, expect, it } from "vitest";
import capability from "../../src-tauri/capabilities/default.json";

// ARCH-043: the save dialog was missing from the Tauri capability, so every
// export (PDF, PNG, job bundle) failed in the installed app while the browser
// preview, which mocks the dialogs, showed nothing wrong. Every dialog
// function the UI imports needs its permission.
const sources = import.meta.glob(["../**/*.svelte", "../**/*.ts"], {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

/** The plugin command each imported function invokes (plugin-dialog 2.x dist-js). */
const COMMAND: Record<string, string> = { open: "open", save: "save", message: "message", ask: "message", confirm: "message" };

describe("Tauri capability", () => {
  it("allows every dialog the UI opens", () => {
    const used = new Set<string>();
    for (const [file, text] of Object.entries(sources)) {
      if (file.endsWith(".test.ts")) continue;
      for (const m of text.matchAll(/import\s*\{([^}]*)\}\s*from\s*["']@tauri-apps\/plugin-dialog["']/g)) {
        for (const name of m[1].split(",").map((s) => s.trim().split(/\s+as\s+/)[0])) {
          if (name) used.add(COMMAND[name] ?? name);
        }
      }
    }
    // window.confirm goes through the dialog plugin's message box.
    used.add("message");
    expect(used.has("save"), "the export dialogs use save()").toBe(true);
    const missing = [...used].filter((c) => !capability.permissions.includes(`dialog:allow-${c}`));
    expect(missing).toEqual([]);
  });
});
