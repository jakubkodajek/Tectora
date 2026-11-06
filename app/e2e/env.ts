// Where the E2E run keeps its library and settings, and which ports it uses.
// Shared by the Playwright config, the server script and the tests.
import path from "node:path";
import { fileURLToPath } from "node:url";

export const APP_DIR = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
export const REPO_DIR = path.dirname(APP_DIR);
export const E2E_DIR = path.join(REPO_DIR, "target", "e2e");
/** The job library the ui_bridge and the MCP server work on. */
export const LIBRARY_DIR = path.join(E2E_DIR, "library");
/** TECTORA_CONFIG_DIR: the settings file the bridge and the MCP server share. */
export const CONFIG_DIR = path.join(E2E_DIR, "config");
/** Scratch files for import / export / attachments. */
export const FILES_DIR = path.join(E2E_DIR, "files");
export const VITE_PORT = Number(process.env.TECTORA_E2E_PORT ?? 1440);
export const BRIDGE_PORT = Number(process.env.TECTORA_E2E_BRIDGE_PORT ?? 7890);
export const BRIDGE_URL = `http://127.0.0.1:${BRIDGE_PORT}`;
const exe = process.platform === "win32" ? ".exe" : "";
export const TECTORA_BIN = path.join(process.env.CARGO_TARGET_DIR ?? path.join(REPO_DIR, "target"), "debug", `tectora${exe}`);
