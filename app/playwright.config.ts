// End-to-end tests of the whole UI in Chromium against the real Rust core:
// the browser preview (ui_bridge + Vite) on its own ports and library, plus
// the `tectora mcp` server for the AI flows. Run: `npm run e2e`.
import { defineConfig } from "@playwright/test";
import { VITE_PORT } from "./e2e/env";

const ci = !!process.env.CI;

export default defineConfig({
  testDir: "e2e",
  testMatch: "**/*.e2e.ts",
  // One shared library and settings file: tests run one after another.
  workers: 1,
  fullyParallel: false,
  forbidOnly: ci,
  retries: ci ? 1 : 0,
  timeout: 45_000,
  expect: { timeout: 8_000 },
  reporter: ci ? [["github"], ["list"], ["html", { open: "never", outputFolder: "../target/e2e-report" }]] : [["list"]],
  outputDir: "../target/e2e-results",
  use: {
    baseURL: `http://localhost:${VITE_PORT}`,
    viewport: { width: 1360, height: 860 },
    locale: "cs-CZ",
    trace: ci ? "on-first-retry" : "off",
    screenshot: "only-on-failure",
  },
  webServer: {
    command: "node e2e/serve.mjs",
    url: `http://localhost:${VITE_PORT}/preview.html`,
    reuseExistingServer: false,
    timeout: 600_000,
    stdout: "pipe",
    stderr: "pipe",
  },
});
