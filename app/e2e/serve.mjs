// Starts the browser preview for the E2E run on its own ports, with an empty
// library and settings folder in the run's folder, and builds the `tectora`
// CLI the MCP tests talk to. Started by playwright.config.ts (webServer),
// which claims the run's slot (e2e/env.ts) and passes TECTORA_E2E_DIR,
// TECTORA_E2E_PORT and TECTORA_E2E_BRIDGE_PORT down.
import { spawn, spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const app = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const repo = path.dirname(app);
const e2e = path.resolve(repo, process.env.TECTORA_E2E_DIR ?? path.join("target", "e2e"));
const win = process.platform === "win32";
const MARK = ".tectora-e2e";

// Only ever empty a folder this harness made (or a target/e2e* run folder): a
// typo in TECTORA_E2E_DIR must not wipe someone's files.
const rel = path.relative(path.join(repo, "target"), e2e);
const inTarget = /^e2e[^\\/]*$/.test(rel) && rel !== "e2e-slots";
if (fs.existsSync(e2e) && !inTarget && !fs.existsSync(path.join(e2e, MARK)) && fs.readdirSync(e2e).length > 0) {
  console.error(`TECTORA_E2E_DIR=${e2e} is not empty and was not made by the E2E harness; refusing to empty it`);
  process.exit(1);
}
fs.rmSync(e2e, { recursive: true, force: true });
fs.mkdirSync(e2e, { recursive: true });
fs.writeFileSync(path.join(e2e, MARK), "");
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
