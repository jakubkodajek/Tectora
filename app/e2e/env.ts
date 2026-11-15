// Where the E2E run keeps its library and settings, and which ports it uses.
// Shared by the Playwright config, the server script and the tests.
//
// Several runs can share one machine (TEST-009): the first process that loads
// this file (the Playwright runner, through playwright.config.ts) claims a free
// run slot under target/e2e-slots and writes it to the environment, so the web
// server and every worker it starts use the same folder and ports.
//
//   slot 0: target/e2e,     Vite 1440,     bridge 7890      (the old fixed values)
//   slot n: target/e2e-<n>, Vite 1440 + n, bridge 7890 + n
//
// TECTORA_E2E_DIR, TECTORA_E2E_PORT and TECTORA_E2E_BRIDGE_PORT override the
// slot's values one by one; TECTORA_E2E_SLOT picks a slot without claiming it.
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

export const APP_DIR = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
export const REPO_DIR = path.dirname(APP_DIR);

const MAX_SLOTS = 32;

function alive(pid: number): boolean {
  if (!Number.isInteger(pid) || pid <= 0) return false;
  try {
    process.kill(pid, 0);
    return true;
  } catch (e) {
    // EPERM: the process exists but belongs to someone else.
    return (e as NodeJS.ErrnoException).code === "EPERM";
  }
}

/**
 * Claims the lowest free slot: target/e2e-slots/<n>.pid is created
 * exclusively and holds the runner's pid. A file whose process has gone is
 * taken over. The file is removed when this process exits.
 */
export function claimSlot(slotsDir = path.join(REPO_DIR, "target", "e2e-slots"), pid = process.pid): number {
  fs.mkdirSync(slotsDir, { recursive: true });
  for (let n = 0; n < MAX_SLOTS; n++) {
    const file = path.join(slotsDir, `${n}.pid`);
    for (let attempt = 0; attempt < 2; attempt++) {
      try {
        fs.writeFileSync(file, String(pid), { flag: "wx" });
        if (pid === process.pid) {
          process.on("exit", () => {
            try {
              if (fs.readFileSync(file, "utf8") === String(pid)) fs.unlinkSync(file);
            } catch {
              // Already gone.
            }
          });
        }
        return n;
      } catch (e) {
        if ((e as NodeJS.ErrnoException).code !== "EEXIST") throw e;
        let owner = NaN;
        try {
          owner = Number(fs.readFileSync(file, "utf8").trim());
        } catch {
          // Removed in between: try again.
        }
        if (alive(owner)) break;
        fs.rmSync(file, { force: true });
      }
    }
  }
  throw new Error(`no free E2E slot in ${slotsDir} (${MAX_SLOTS} runs at once?)`);
}

/** The folder and ports of slot `n`, before any override from the environment. */
export function slotDefaults(n: number) {
  return {
    dir: path.join(REPO_DIR, "target", n === 0 ? "e2e" : `e2e-${n}`),
    vitePort: 1440 + n,
    bridgePort: 7890 + n,
    results: path.join(REPO_DIR, "target", n === 0 ? "e2e-results" : `e2e-results-${n}`),
    report: path.join(REPO_DIR, "target", n === 0 ? "e2e-report" : `e2e-report-${n}`),
  };
}

if (process.env.TECTORA_E2E_SLOT === undefined) process.env.TECTORA_E2E_SLOT = String(claimSlot());
export const SLOT = Number(process.env.TECTORA_E2E_SLOT);
const slot = slotDefaults(SLOT);
process.env.TECTORA_E2E_DIR ??= slot.dir;
process.env.TECTORA_E2E_PORT ??= String(slot.vitePort);
process.env.TECTORA_E2E_BRIDGE_PORT ??= String(slot.bridgePort);

export const E2E_DIR = path.resolve(REPO_DIR, process.env.TECTORA_E2E_DIR!);
/** The job library the ui_bridge and the MCP server work on. */
export const LIBRARY_DIR = path.join(E2E_DIR, "library");
/** TECTORA_CONFIG_DIR: the settings file the bridge and the MCP server share. */
export const CONFIG_DIR = path.join(E2E_DIR, "config");
/** Scratch files for import / export / attachments. */
export const FILES_DIR = path.join(E2E_DIR, "files");
export const VITE_PORT = Number(process.env.TECTORA_E2E_PORT);
export const BRIDGE_PORT = Number(process.env.TECTORA_E2E_BRIDGE_PORT);
export const BRIDGE_URL = `http://127.0.0.1:${BRIDGE_PORT}`;
/** Playwright's output and HTML report folders of this run. */
export const RESULTS_DIR = slot.results;
export const REPORT_DIR = slot.report;
const exe = process.platform === "win32" ? ".exe" : "";
export const TECTORA_BIN = path.join(process.env.CARGO_TARGET_DIR ?? path.join(REPO_DIR, "target"), "debug", `tectora${exe}`);
