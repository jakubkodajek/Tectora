// Shared E2E fixtures: a clean library and settings file per test, the app
// page with Tauri's dialog behaviour, and helpers to drive the ui_bridge and
// the MCP server directly.
import { test as base, expect, type Page } from "@playwright/test";
import fs from "node:fs";
import path from "node:path";
import { BRIDGE_URL, CONFIG_DIR, FILES_DIR, LIBRARY_DIR } from "./env";
import { Mcp } from "./mcp";

export { expect };

export type Locale = "cs" | "sk" | "en" | "de";

/** Calls a Tauri command on the ui_bridge, as the app does. */
export async function bridge<T = any>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  const res = await fetch(`${BRIDGE_URL}/${encodeURIComponent(cmd)}`, { method: "POST", body: JSON.stringify(args) });
  const body = (await res.json()) as { ok?: T; err?: unknown };
  if ("err" in body) throw new Error(`${cmd}: ${JSON.stringify(body.err)}`);
  return body.ok as T;
}

/**
 * Empties the library and the settings folder, points the bridge back at the
 * test library (a test may switch the jobs folder) and drops pending file
 * events.
 */
export async function resetData() {
  for (const dir of [LIBRARY_DIR, CONFIG_DIR, FILES_DIR]) {
    fs.rmSync(dir, { recursive: true, force: true });
    fs.mkdirSync(dir, { recursive: true });
  }
  await bridge("__reset");
}

/** Writes a scratch file for attachments / imports and returns its path. */
export function scratchFile(name: string, content: string | Buffer): string {
  const p = path.join(FILES_DIR, name);
  fs.mkdirSync(path.dirname(p), { recursive: true });
  fs.writeFileSync(p, content);
  return p;
}

export const loadProject = (id: string) => bridge<any>("load_project", { id });
export const listProjects = async () => (await bridge<{ projects: any[] }>("list_projects")).projects;

/** Creates a job from a built-in example over the bridge; returns its id. */
export const createExample = (exampleId: string, name?: string) =>
  bridge<string>("create_from_example", { exampleId, name: name ?? null });

/** Waits until the stored project satisfies `check` (autosave runs 0.5 s after an edit). */
export async function waitForProject(id: string, check: (p: any) => boolean, timeout = 8000): Promise<any> {
  const end = Date.now() + timeout;
  let last: any;
  while (Date.now() < end) {
    last = await loadProject(id);
    if (check(last)) return last;
    await new Promise((r) => setTimeout(r, 150));
  }
  throw new Error(`project ${id} did not reach the expected state:\n${JSON.stringify(last, null, 1).slice(0, 3000)}`);
}

// Tauri's dialog plugin makes window.confirm / alert / prompt asynchronous (a
// Promise), and the preview's tauri-mock asks for file paths with prompt().
// The tests answer both from a queue; every dialog is recorded.
const INIT = `
  window.__e2e = { confirm: [], paths: [], dialogs: [] };
  window.confirm = (m) => {
    window.__e2e.dialogs.push({ kind: "confirm", message: String(m) });
    const a = window.__e2e.confirm.length ? window.__e2e.confirm.shift() : true;
    return Promise.resolve(a);
  };
  window.alert = (m) => { window.__e2e.dialogs.push({ kind: "alert", message: String(m) }); return Promise.resolve(); };
  window.prompt = (m) => {
    window.__e2e.dialogs.push({ kind: "prompt", message: String(m) });
    return window.__e2e.paths.length ? window.__e2e.paths.shift() : null;
  };
`;

export class App {
  readonly errors: string[] = [];
  constructor(readonly page: Page) {}

  /** Opens the app (library screen) in `locale`. */
  async open(opts: { locale?: Locale; theme?: "light" | "dark"; prefs?: Record<string, unknown> } = {}) {
    const { locale = "cs", theme, prefs } = opts;
    await this.page.addInitScript(
      ([l, th, pr]) => {
        if (sessionStorage.getItem("e2e-init")) return;
        sessionStorage.setItem("e2e-init", "1");
        localStorage.clear();
        localStorage.setItem("tectora.locale", l as string);
        if (th) localStorage.setItem("tectora.theme", th as string);
        if (pr) localStorage.setItem("tectora.prefs", pr as string);
      },
      [locale, theme ?? "", prefs ? JSON.stringify(prefs) : ""],
    );
    await this.page.goto(`/preview.html?bridge=${encodeURIComponent(BRIDGE_URL)}`);
    await expect(this.page.locator("header .brand, .editor")).toBeVisible();
  }

  /** Answers the next confirm() dialogs (Tauri: async). */
  async answerConfirm(...answers: boolean[]) {
    await this.page.evaluate((a) => (window as any).__e2e.confirm.push(...a), answers);
  }

  /** Answers the next native file dialogs (open / save) with these paths; null cancels. */
  async answerFileDialog(...paths: (string | null)[]) {
    await this.page.evaluate((p) => (window as any).__e2e.paths.push(...p), paths);
  }

  async dialogs(): Promise<{ kind: string; message: string }[]> {
    return this.page.evaluate(() => (window as any).__e2e.dialogs);
  }

  /** Opens a job from the library list by its name (double-click on the row). */
  async openJob(name: string) {
    await this.page.getByRole("row").filter({ hasText: name }).first().dblclick();
    await expect(this.page.locator(".editor .statusbar")).toBeVisible();
  }

  /** The editor's save state has settled on "saved". */
  async saved() {
    await expect(this.page.locator(".statusbar .save")).toHaveText(/Uloženo|Saved|Gespeichert|Uložené/, { timeout: 8000 });
  }

  toolbar(name: string | RegExp) {
    return this.page.getByRole("toolbar").first().getByRole("button", { name });
  }

  toasts() {
    return this.page.locator(".stack .toast");
  }
}

type Fixtures = { app: App; mcp: Mcp };

export const test = base.extend<Fixtures>({
  app: async ({ page }, use, testInfo) => {
    await resetData();
    await page.addInitScript(INIT);
    const app = new App(page);
    page.on("pageerror", (e) => app.errors.push(`pageerror: ${e.message}`));
    page.on("console", (m) => {
      if (m.type() === "error") app.errors.push(`console: ${m.text()}`);
    });
    await use(app);
    // A page error is always a bug; tests that expect one clear the list.
    if (testInfo.status === testInfo.expectedStatus) {
      expect(app.errors, "page or console errors during the test").toEqual([]);
    }
  },
  mcp: async ({}, use) => {
    const m = await Mcp.start("Claude");
    await use(m);
    m.stop();
  },
});
