import { defineConfig, type Plugin } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// Browser preview (`npm run preview`): serve preview.html at "/", so the app
// runs without Tauri against the ui_bridge. Dev server only; builds are unchanged.
const previewRoot: Plugin = {
  name: "tectora-preview-root",
  apply: "serve",
  configureServer(server) {
    if (!process.env.TECTORA_PREVIEW) return;
    server.middlewares.use((req, _res, next) => {
      if (req.url === "/" || req.url === "/index.html") req.url = "/preview.html";
      next();
    });
  },
};

// Tauri expects a fixed port and no screen clearing.
export default defineConfig({
  plugins: [svelte(), previewRoot],
  clearScreen: false,
  server: { port: 5173, strictPort: true },
  build: { target: "es2022", chunkSizeWarningLimit: 1200 },
});
