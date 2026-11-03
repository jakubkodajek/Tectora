// Dev-only stand-in for the Tauri runtime, so the UI runs in a plain browser
// (`npm run preview`, page `/preview.html`). Every `invoke` is sent to the
// ui_bridge example (crates/tectora-core/examples/ui_bridge.rs), which answers
// the same commands as app/src-tauri over a real library folder. File changes
// in that folder (for example from `tectora mcp --dir <same folder>`) arrive
// as the usual `projects-changed` / `live-status` events by polling.
// Never imported by index.html, so it is not part of the app build.

type Callback = (payload: unknown) => void;

// Another bridge port: /preview.html?bridge=http://127.0.0.1:7879
const BRIDGE = new URLSearchParams(location.search).get("bridge") ?? "http://127.0.0.1:7878";
const POLL_MS = 700;

const callbacks = new Map<number, Callback>();
const listeners = new Map<number, { event: string; handler: number }>();
let nextId = 1;

async function bridge(cmd: string, args: unknown): Promise<unknown> {
  let res: Response;
  try {
    res = await fetch(`${BRIDGE}/${encodeURIComponent(cmd)}`, {
      method: "POST",
      body: JSON.stringify(args ?? {}),
    });
  } catch {
    throw `ui_bridge is not running at ${BRIDGE} (npm run preview starts it)`;
  }
  const body = (await res.json()) as { ok?: unknown; err?: unknown };
  if ("err" in body) {
    console.warn(`[preview] ${cmd}:`, body.err);
    throw body.err;
  }
  return body.ok;
}

async function dialog(cmd: string, args: Record<string, unknown>): Promise<unknown> {
  const opts = (args.options ?? args) as Record<string, unknown>;
  const text = String(opts.message ?? opts.title ?? "");
  switch (cmd) {
    case "plugin:dialog|open":
      return prompt("Preview: path of the file to open (empty = cancel)") || null;
    case "plugin:dialog|save":
      return prompt("Preview: path to save to (empty = cancel)", String(opts.defaultPath ?? "")) || null;
    case "plugin:dialog|message":
      alert(text);
      return "Ok";
    case "plugin:dialog|ask":
    case "plugin:dialog|confirm":
      return confirm(text);
    default:
      return null;
  }
}

async function invoke(cmd: string, args: Record<string, unknown> = {}): Promise<unknown> {
  if (cmd === "plugin:event|listen") {
    const id = nextId++;
    listeners.set(id, { event: String(args.event), handler: Number(args.handler) });
    return id;
  }
  if (cmd === "plugin:event|unlisten") {
    listeners.delete(Number(args.eventId));
    return null;
  }
  if (cmd.startsWith("plugin:dialog|")) return dialog(cmd, args);
  if (cmd.startsWith("plugin:")) return null;
  return bridge(cmd, args);
}

async function poll(): Promise<void> {
  try {
    const events = (await bridge("__events", {})) as { event: string; payload: unknown }[];
    for (const e of events) {
      for (const [id, l] of listeners) {
        if (l.event === e.event) callbacks.get(l.handler)?.({ event: e.event, id, payload: e.payload });
      }
    }
  } catch {
    // Bridge restarting; try again on the next tick.
  }
  setTimeout(poll, POLL_MS);
}

const w = window as unknown as Record<string, unknown>;
w.__TAURI_INTERNALS__ = {
  metadata: {
    currentWindow: { label: "main" },
    currentWebview: { label: "main", windowLabel: "main" },
  },
  transformCallback: (cb: Callback) => {
    const id = nextId++;
    callbacks.set(id, cb);
    return id;
  },
  unregisterCallback: (id: number) => callbacks.delete(id),
  convertFileSrc: (path: string) => path,
  invoke,
};
w.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
  unregisterListener: (_event: string, id: number) => listeners.delete(id),
};

// The preview library is a separate folder; say so in the console once.
bridge("projects_dir", {}).then(
  (dir) => console.info(`[preview] library: ${dir}`),
  (e) => console.error(`[preview] ${e}`),
);
void poll();
