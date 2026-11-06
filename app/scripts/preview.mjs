// Starts the browser preview: the ui_bridge (Rust, real tectora-core) and
// Vite on port 1430, where / serves preview.html. Ctrl+C stops both.
//
//   npm run preview                 library: <repo>/target/ui-preview-library
//   TECTORA_PREVIEW_DIR=/some/dir npm run preview
//   TECTORA_PREVIEW_PORT / TECTORA_BRIDGE_PORT: other ports (Vite / ui_bridge)
//
// Point `tectora mcp --dir` at the same folder to watch AI edits live.
import { spawn, spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";

const app = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const repo = path.dirname(app);
const dir = path.resolve(repo, process.env.TECTORA_PREVIEW_DIR ?? "target/ui-preview-library");
const port = process.env.TECTORA_PREVIEW_PORT ?? "1430";
// The page picks another bridge with /preview.html?bridge=http://127.0.0.1:<port>.
const bridgePort = process.env.TECTORA_BRIDGE_PORT ?? "7878";
const win = process.platform === "win32";

// Build first, then run the binaries directly, so stopping this script stops
// them too (npx and cargo run would leave their children running).
const build = spawnSync("cargo", ["build", "-q", "-p", "tectora-core", "--example", "ui_bridge"], {
  cwd: repo,
  stdio: "inherit",
  shell: win,
});
if (build.status !== 0) process.exit(build.status ?? 1);
const target = process.env.CARGO_TARGET_DIR ?? path.join(repo, "target");
const bridge = path.join(target, "debug", "examples", win ? "ui_bridge.exe" : "ui_bridge");
const vite = path.join(app, "node_modules", "vite", "bin", "vite.js");

const children = [
  spawn(bridge, [dir, bridgePort], { cwd: repo, stdio: "inherit" }),
  spawn(process.execPath, [vite, "--port", port, "--strictPort"], {
    cwd: app,
    stdio: "inherit",
    env: { ...process.env, TECTORA_PREVIEW: "1" },
  }),
];

const stop = (code = 0) => {
  for (const c of children) if (!c.killed) c.kill();
  process.exit(code);
};
for (const c of children) c.on("exit", (code) => stop(code ?? 0));
process.on("SIGINT", () => stop(0));
process.on("SIGTERM", () => stop(0));
