// Shared E2E fixtures: a clean library and settings file per test, the app
// page with Tauri's dialog behaviour, and helpers to drive the ui_bridge and
// the MCP server directly.
import { test as base, expect, type Page } from "@playwright/test";
import fs from "node:fs";
import path from "node:path";
import http from "node:http";
import zlib from "node:zlib";
import { BRIDGE_URL, CONFIG_DIR, FILES_DIR, LIBRARY_DIR, RELAY_PORT } from "./env";
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

/**
 * Fails on any sighting of `locator` during `ms` (sampled every 20 ms; a
 * toast stays seconds). `expect(..).toHaveCount(0)` passes once a toast
 * that was there has gone, so it cannot tell "never shown" (audit
 * 2026-11-23 TEST-1002): wait for an earlier toast to go first.
 */
export async function noToast(locator: import("@playwright/test").Locator, ms = 1500, what = "a toast that must not be shown") {
  const end = Date.now() + ms;
  while (Date.now() < end) {
    const n = await locator.count();
    if (n > 0) throw new Error(`${what} was shown: ${JSON.stringify(await locator.allTextContents())}`);
    await new Promise((r) => setTimeout(r, 20));
  }
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

  /**
   * Opens the app (library screen) in `locale`. The wall and dormer bars'
   * "More" rows start open, so tests reach every field; pass
   * `prefs: { wallMore: false }` for the closed default a new user sees.
   */
  async open(opts: { locale?: Locale; theme?: "light" | "dark"; prefs?: Record<string, unknown> } = {}) {
    const { locale = "cs", theme } = opts;
    const prefs = { wallMore: true, dormerMore: true, ...opts.prefs };
    await this.page.addInitScript(
      ([l, th, pr]) => {
        if (sessionStorage.getItem("e2e-init")) return;
        sessionStorage.setItem("e2e-init", "1");
        localStorage.clear();
        localStorage.setItem("tectora.locale", l as string);
        if (th) localStorage.setItem("tectora.theme", th as string);
        if (pr) localStorage.setItem("tectora.prefs", pr as string);
      },
      [locale, theme ?? "", JSON.stringify(prefs)],
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

/** What one upload to the relay fixture looked like. */
export interface Upload {
  body: { v: number; key: string; id: string; file_name: string; size: number; meta: Record<string, any>; zip_b64: string };
  /** The ZIP's entries by name (stored and deflated entries). */
  zip: Map<string, Buffer>;
}

/** The entries of a ZIP, read from its central directory. */
export function unzip(buf: Buffer): Map<string, Buffer> {
  const out = new Map<string, Buffer>();
  let end = buf.length - 22;
  while (end >= 0 && buf.readUInt32LE(end) !== 0x06054b50) end--;
  if (end < 0) throw new Error("not a zip");
  const count = buf.readUInt16LE(end + 10);
  let at = buf.readUInt32LE(end + 16);
  for (let i = 0; i < count; i++) {
    if (buf.readUInt32LE(at) !== 0x02014b50) throw new Error("bad central directory");
    const method = buf.readUInt16LE(at + 10);
    const size = buf.readUInt32LE(at + 20);
    const n = buf.readUInt16LE(at + 28);
    const m = buf.readUInt16LE(at + 30);
    const k = buf.readUInt16LE(at + 32);
    const local = buf.readUInt32LE(at + 42);
    const name = buf.subarray(at + 46, at + 46 + n).toString("utf8");
    const start = local + 30 + buf.readUInt16LE(local + 26) + buf.readUInt16LE(local + 28);
    const data = buf.subarray(start, start + size);
    out.set(name, method === 8 ? zlib.inflateRawSync(data) : Buffer.from(data));
    at += 46 + n + m + k;
  }
  return out;
}

/**
 * The relay of the report tests: a Node `http` server on the run's relay
 * port that answers like Apps Script (POST -> 302 -> GET -> JSON). `refuse()`
 * closes the port (connection refused, the app is "offline"), `accept()`
 * opens it again. `hold()` makes the next upload wait until `release()` so a
 * test can look at the dialog while a send is under way. Waits are on
 * promises (`nextUpload`), never on time.
 */
export class Relay {
  readonly uploads: Upload[] = [];
  private server: http.Server | null = null;
  private waiters: ((u: Upload) => void)[] = [];
  private gate: Promise<void> | null = null;
  private open!: () => void;
  private connections = new Set<import("node:net").Socket>();
  /** The answer's `error`, or null for success. */
  answer: { ok: false; error: string; retry_after_s?: number } | null = null;

  async accept() {
    if (this.server) return;
    const server = http.createServer(async (req, res) => {
      const chunks: Buffer[] = [];
      for await (const c of req) chunks.push(c as Buffer);
      if (req.method === "POST") {
        const body = JSON.parse(Buffer.concat(chunks).toString("utf8"));
        const upload: Upload = { body, zip: unzip(Buffer.from(body.zip_b64, "base64")) };
        if (this.gate) await this.gate;
        if (!this.answer) {
          this.uploads.push(upload);
          const w = this.waiters.shift();
          w?.(upload);
        }
        res.writeHead(302, { Location: "/macros/echo?k=1", Connection: "close" }).end();
      } else {
        const payload = this.answer ?? { ok: true, file_id: "E2EFILE", issue_url: "https://github.com/jakubkodajek/Tectora/issues/1" };
        res.writeHead(200, { "Content-Type": "application/json", Connection: "close" }).end(JSON.stringify(payload));
      }
    });
    server.on("connection", (s) => {
      this.connections.add(s);
      s.on("close", () => this.connections.delete(s));
    });
    await new Promise<void>((ok, bad) => server.once("error", bad).listen(RELAY_PORT, "127.0.0.1", ok));
    this.server = server;
  }

  async refuse() {
    const s = this.server;
    if (!s) return;
    this.server = null;
    for (const c of this.connections) c.destroy();
    await new Promise<void>((r) => s.close(() => r()));
  }

  /** Resolves with the next upload that arrives (or the one already there that was not awaited). */
  nextUpload(): Promise<Upload> {
    return new Promise((r) => this.waiters.push(r));
  }

  hold() {
    this.gate = new Promise<void>((r) => (this.open = r));
  }

  release() {
    this.open?.();
    this.gate = null;
  }
}

type Fixtures = { app: App; mcp: Mcp; relay: Relay };

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
  relay: async ({}, use) => {
    const r = new Relay();
    await r.accept();
    await use(r);
    r.release();
    await r.refuse();
  },
  mcp: async ({}, use) => {
    const m = await Mcp.start("Claude");
    await use(m);
    m.stop();
  },
});
