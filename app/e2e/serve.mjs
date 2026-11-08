// Starts the browser preview for the E2E run on its own ports, with an empty
// library and settings folder under target/e2e, and builds the `tectora` CLI
// the MCP tests talk to. Started by playwright.config.ts (webServer).
import { spawn, spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const app = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const repo = path.dirname(app);
const e2e = path.join(repo, "target", "e2e");
const win = process.platform === "win32";

fs.rmSync(e2e, { recursive: true, force: true });
for (const d of ["library", "config", "files"]) fs.mkdirSync(path.join(e2e, d), { recursive: true });

const build = spawnSync("cargo", ["build", "-q", "-p", "tectora-cli"], { cwd: repo, stdio: "inherit", shell: win });
if (build.status !== 0) process.exit(build.status ?? 1);

const child = spawn(process.execPath, [path.join(app, "scripts", "preview.mjs")], {
  cwd: app,
  stdio: "inherit",
  env: {
    ...process.env,
    TECTORA_PREVIEW_DIR: path.join(e2e, "library"),
    TECTORA_CONFIG_DIR: path.join(e2e, "config"),
    TECTORA_PREVIEW_PORT: process.env.TECTORA_E2E_PORT ?? "1440",
    TECTORA_BRIDGE_PORT: process.env.TECTORA_E2E_BRIDGE_PORT ?? "7890",
  },
});
child.on("exit", (code) => process.exit(code ?? 0));
for (const sig of ["SIGINT", "SIGTERM"]) process.on(sig, () => child.kill(sig));
